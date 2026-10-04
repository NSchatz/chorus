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
layouts and the Wi-Fi refusal, state-file format 2), and for per-room sound
`docs/decisions/0081-per-room-sound-in-the-catalog-and-on-the-wire.md`, and
for the speakers `docs/decisions/0106-speakers-adopted-named-and-assigned-rooms.md`,
and for player sources and what is playing
`docs/decisions/0119-player-sources-player-ports-and-now-playing.md`.

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
| `quiet_enabled` (v2) | whether the room's quiet hours are switched on, `true` unless switched off; the windows are stored either way |
| `bond` (v2) | the room's bonded set: endpoints each playing a channel role, or none |
| `sound` (v2, goal 12) | tone, loudness, night mode and speech enhancement |
| `bass_management` (v2, goal 12) | crossover, sub level and sub polarity, in force when the set has an `LFE` member |
| `room_eq` (v2, goal 12) | up to 8 room-correction filters and whether they are applied |

**The clamp rule (v2).** A room's **effective limit** is
`min(limit, the cap of every quiet-hours window active now)`, and `limit` alone
while the room's quiet hours are switched off (`quiet_enabled` false). Every volume path
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
stream, the default), `none`, `chime:<name>`, `line-in:<endpoint>/<input>` or
(goal 16) `player:<id>`, one of the server's network media players.
The spelling is the catalog's; which chimes exist, which inputs are offered and
which players the server runs is the server runtime's, and the state lists the
offered inputs as `inputs`.

**A line-in plays in any number of groups** (goal 17). `take` with a
`line-in:` source that another formed group already plays is not refused and
moves nothing: both groups play it, from the one start of the input, and each
shows it as its `source`. It stays started while any group plays it.

**`stored:<id>` is an alarm's source, never a group's** (goal 17). It names a
stored source (below): a stream URL or a Spotify URI a person entered once.
An `alarm_set` may carry it; a `take` that does is refused, field `source`
(`'stored:morning-radio' is a stored source, which only an alarm plays
(alarm_set with this source); a take cannot name one`). There is no command
that plays a URL in a room (brief section 4.8, ADR 0124): the group of a
ringing alarm plays the `player:<id>` the server gave it, and says so.

**A player plays in at most one group** (goal 16). A `take` that names a player
another formed group is playing is refused, field `source`, naming that group;
so is one naming a player the server does not run (`there is no player 'p7' on
this server; its players are p0, p1`: the players are `p0` to `p<N-1>` of
`--players N`). A player follows its group as any source does when a live
group forms or dissolves. The one place a source is otherwise copied, `take`
pushing rooms out of the target's group, keeps the player with the target and
leaves the rooms pushed out playing `none`; when that `take` gives the target
another source, the player goes with the rooms pushed out instead.

What a player is playing is the group's **now-playing record** (goal 16): its
title, artist, album, artwork URL, duration and whether it is playing, paused
or buffering, and what is driving the player (`via`). It is a fact about now:
the server's runtime sets it, no command does, and it is never persisted. A
group has one only while its source is a player source; it moves with the
source when groups re-form and is gone the moment the group plays anything
else or is no longer formed. (goal 17) One more group has a record, which
the room model writes itself: a group playing a line-in labelled as a
`streamer` (below) shows the label's name as `title`, `state` `playing` and
`via` `streamer`, for as long as it plays that input.

A **speaker** (v2, goal 14) is an endpoint this server has adopted: the id
its audio sessions authenticate as, pinned to its key on its first handshake
(`docs/protocol.md`, "Adoption: trust on first use"). The server keeps one
record per adopted id: a `name` (`Speaker ` and the last four characters of
the id until a person names it), whether it was `named`, and the `room` it
was assigned, or none. Nobody has to do anything for a speaker to be listed:
that is what "auto-adopt on the LAN, name it in the app" (K92) means here.
See "Speakers: adoption, names and rooms" below.

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
| `quiet_hours` | `zone`, `windows`: `[{"days","start","end","limit"}]`, at most 8 | replaces the room's windows; `[]` removes them. Days are never empty. Does not change whether they are switched on |
| `quiet_hours_enabled` | `zone`, `enabled` (`true` or `false`) | switches the room's quiet hours on or off and keeps its windows. They are on by default. Off: no window caps the room, and nothing is raised. On again inside a window: the cap applies at once and the volume is pulled down to it |
| `alarm_set` | `alarm`, `target` (a room or a saved group), `time`, `days` (empty is once), `source`, `volume`, `ramp_s` (0 to 600), `duration_min` (0 to 720; 0 plays until stopped), `enabled` | creates or replaces an alarm |
| `alarm_delete` | `alarm` | forgets it |
| `alarm_stop` | `alarm` | stops it ringing; one not ringing is not an error |
| `sleep` | `target` (a room or a formed group), `minutes` (0 to 720) | asks for a sleep timer; `0` cancels |
| `autoplay` | `input` (`<endpoint>/<input>`), `target` (a room or a saved group), `enabled` | creates or replaces the rule for that input |
| `sound` | `zone`, optional `bass`, `treble` (whole dB, -10 to 10), `loudness`, `night`, `speech` (booleans) | changes the fields it carries; an absent one keeps what the room had |
| `bass_management` | `zone`, optional `crossover_hz` (40 to 200), `sub_level_db` (-12.00 to 6.00, two places), `sub_polarity` (`"normal"` or `"inverted"`) | the same, partial |
| `room_eq` | `zone`, optional `filters`: `[{"freq_hz","gain_db","q"}]` at most 8, optional `enabled` | `filters` replaces the room's (`[]` clears them); `enabled` turns them on or off and keeps them |
| `source_store` | `id`, `kind` (`"url"` or `"spotify"`), `value`, `name` | (goal 17) stores or replaces a stored source, which an alarm names as `stored:<id>`; below |
| `source_forget` | `id` | (goal 17) forgets a stored source; refused while an alarm plays it |
| `input_label` | `input` (`<endpoint>/<input>`), `name`, `role` (`"line-in"` or `"streamer"`) | (goal 17) names an input and says what is wired to it; an empty `name` with `role` `line-in` removes the label; below |
| `soloist_restart` | none | (goal 17) every Spotify Soloist receiver's supervisor reads its binary again and starts again; refused (`no-receivers`) on a server started without `--soloist-receivers`; below |
| `playback` | `target` (a room, a saved group or a formed group), `action` (`"pause"`, `"resume"`, `"next"` or `"previous"`) | (goal 17) forwarded to the Spotify receiver the target's group is playing; refused for a group that plays anything else; below |
| `announce` | `target` (a room, a saved group or a formed group), `url`, optional `volume` | (goal 18) plays a clip from a configured origin in the target's group, then puts back what the group played; refused for a URL from any other origin; below |

```json
{"v":2,"t":"bond","zone":"living","members":[{"endpoint":"endpoint-a","role":"FL"},{"endpoint":"endpoint-b","role":"FR"}]}
{"v":2,"t":"take","target":"downstairs","source":"line-in:endpoint-c/line-1"}
{"v":2,"t":"quiet_hours","zone":"bedroom","windows":[{"days":["mon","tue","wed","thu","fri"],"start":"22:00","end":"07:00","limit":0.250}]}
{"v":2,"t":"quiet_hours_enabled","zone":"bedroom","enabled":false}
```

**Quiet hours switched off and on.** `quiet_hours_enabled` is one flag per
room (`docs/decisions/0150-quiet-hours-switched-off-and-on-per-room.md`), so
that one switch in a home-automation system can drive it without knowing the
windows. The default is enabled: a room nobody switched off behaves as it
always did. While it is off the room keeps every window exactly as it was set,
the state still says which of them the clock is inside (`active`), and the
room's `effective_limit` is its `limit`. The flag is persisted (state-file
format 7), so a restart inside a window does not bring the cap back.
`quiet_hours` replaces the windows and leaves the flag alone; sending
`enabled` the value it already has is accepted and changes nothing but the
serial. `fixtures/control/v2/quiet_hours_enabled.json` pins the command and
`fixtures/control/v2/state-quiet-disabled.json` a room switched off inside one
of its windows.

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

### Per-room sound (goal 12)

`docs/decisions/0081-per-room-sound-in-the-catalog-and-on-the-wire.md` records
the choices and the citations.

`sound`, `bass_management` and `room_eq` are a room's sound, carried to every
endpoint of the room on the audio wire as `sound` (`docs/protocol.md`, "0x39
sound"), where the endpoint's DSP realises it. None of them is a volume path:
none moves `volume`, `limit` or `effective_limit`, and the clamp rule is
untouched (an endpoint's limiter keeps a DSP boost under the room's limit).
Each is a partial update: only the fields it carries change, and a command with
only `zone` changes nothing and still answers with the state.

| setting | range | default |
|---|---|---|
| `bass`, `treble` | whole dB, -10 to 10 (ASSUMED 1 dB a step) | 0 |
| `loudness` | boolean | `true` (ASSUMED as Sonos's default; not printed in Sonos's own documentation) |
| `night`, `speech` | boolean | `false` |
| `crossover_hz` | 40 to 200 | 80 (THX) |
| `sub_level_db` | -12.00 to 6.00, two places | `0.00` |
| `sub_polarity` | `"normal"`, `"inverted"` | `"normal"` |
| `room_eq.filters` | at most 8; `freq_hz` 20 to 1000, `gain_db` -12.00 to 3.00 (two places), `q` 0.500 to 10.000 (three places) | none |
| `room_eq.enabled` | boolean | `true` |

The `room_eq` bounds are THE room-correction bounds (`ROOM_EQ_*` in
`crates/control/src/sound.rs`, `SOUND_EQ_*` on the wire): the room-correction
fitter emits only filters inside them and the catalog refuses anything outside
them, naming `filters` with the filter and its field in the detail. A decibel
is written with exactly two places and a Q with three, and read back with at
most that many: `0.7071` is refused, not rounded. Bass management is held
whatever the room's set; the state's `active` says whether the set has an
`LFE` member, which is what turns it on.

```json
{"v":2,"t":"sound","zone":"living","bass":3,"treble":-2,"loudness":false,"night":true,"speech":true}
{"v":2,"t":"bass_management","zone":"living","crossover_hz":100,"sub_level_db":-3.50,"sub_polarity":"inverted"}
{"v":2,"t":"room_eq","zone":"living","filters":[{"freq_hz":42,"gain_db":-6.00,"q":4.500},{"freq_hz":120,"gain_db":-3.25,"q":2.000}],"enabled":true}
```

### The TV path: A/V trim, TV upmix, TV autoplay (goal 13)

`docs/decisions/0088-theater-maps-av-trim-and-tv-autoplay.md` records the
choices and the citations.

- **`av_trim`** sets a room's signed A/V trim, `av_trim_ms`, a whole number of
  milliseconds from -100 to 200 (ASSUMED bounds, waiting on the Needs item "The
  three TVs: model, eARC port, optical out and audio menu"). Positive DELAYS
  the room's TV audio (a TV whose picture is slower than its sound); negative
  brings it earlier. It applies only to the TV relay's stamps:
  `play_at = capture_stamp + max(L_floor, L_tv + trim)`, and a trim that asks
  for less than the floor is clamped to it and logged `av-trim-clamped`, never
  refused (`chorus_control::theater::tv_play_at_lead_ns` is the rule, pure). It
  is not a volume path and moves nothing else. Out of range is refused naming
  `av_trim_ms`.
- **`tv_upmix`** is a field of `sound` (partial, as the others): `"off"` (the
  default, ASSUMED) or `"ambient"`. It decides what a theater set's surround
  members play from a stream with no surround channel (a stereo TV): silence,
  or the passive matrix surround `(FL - FR) / sqrt 2`, band-limited and 20 ms
  late (`docs/dsp.md`, "The theater maps"). It travels to the endpoints in the
  `sound` message's theater block, with the set's `fold` bits the server
  derives from the bond (`docs/protocol.md`, 0x39).
- **An autoplay rule's TV fields** (K81): `stop_on_standby` (default `true`):
  for a TV input (kind `optical` or `hdmi_arc`), the TV going to standby stops
  the autoplay at once, without the 30 s hold, and restores what its rooms
  played; `low_latency` (default `true`): the integration track's relay plays
  the TV in low-latency mode when the target is one wired room, and `false`
  keeps it on the slot path. Both are written only when `false`, so a rule
  that leaves them at their defaults has goal 11's bytes.

```json
{"v":2,"t":"av_trim","zone":"living","av_trim_ms":-40}
{"v":2,"t":"sound","zone":"living","tv_upmix":"ambient"}
{"v":2,"t":"autoplay","input":"hub/tv","target":"living","enabled":true,"stop_on_standby":false,"low_latency":false}
```

The state message carries both per room: `"av_trim_ms":0` beside `sound`, and
`"tv_upmix":"off"` inside it.

### The TV relay: when a TV plays in low-latency mode (goal 13)

The server's TV relay (`crates/server/src/tvrelay.rs`; the wire is
`docs/protocol.md` "Low-latency path", the integration's choices
`docs/decisions/0094-the-tv-relay-and-the-low-latency-endpoints.md`) plays a TV input in low-latency mode when, at the
conductor's pass, all of these hold, and otherwise leaves it on its stream slot
(ADR 0079), unchanged:

- the input is a TV's (`optical` or `hdmi_arc`) and streaming from its hub;
- its autoplay rule, if it has one, says `low_latency` (a TV input played by a
  command and no rule is wanted in low-latency mode too: the rule's default,
  ASSUMED for a command, which has no field of its own);
- the group playing it is one room (autoplay's `take` makes it so; K81), and
  that room is wired (`--zone <id>=wireless` is not);
- the server's chunk fits one datagram (`chunk_fits`: stereo at 120 frames
  does; 5.1 at s24 needs 60 frames);
- the hub and every player session of the room advertise
  `capabilities.features` `low_latency` (a Linux endpoint does when wired).

A change of mode is one line: `tv-path mode=low-latency input=<id> room=<id>`,
or `tv-path mode=slot input=<id> group=<id> reason=<why>`, `<why>` one of
`rule`, `grouped`, `wireless`, `chunk-does-not-fit`, `no-player`,
`player-not-capable`, `hub-not-capable`, `refused`. The relay offers the
players first and the hub only when every player accepted, so the hub never
switches to datagrams with nobody listening; a refusal or an offer unanswered
for 2 s (ASSUMED) ends every stream of that play (`tv-path refused ...`) and
keeps it on the slot until what it is made of (its room's sessions, its hub,
its group) changes. While a TV plays in low-latency mode the slot that still
carries its group hears nothing from the hub (its port underruns and counts
it), and the players play the datagrams instead. The relay's lines:
`tv-relay offer`, `tv-relay active`, `tv-relay first-chunk` (the first
chunk's sequence, capture stamp and play-at), `tv-relay lead` (a trim moved
the lead), `av-trim-clamped`, and `tv-relay end ... reason=<why>` with its
counts (received, recovered, unrecoverable, relayed, late, sent, and the
test-only drops). Volume and sound are the endpoints' as on every path: the
room's `room_volume` and `sound` reach them on their sessions.

The server's flags for it:

| flag | default | what |
|---|---|---|
| `--low-latency-port <port>` | the audio port + 1 (ASSUMED); with an ephemeral audio port, ephemeral | the relay's UDP port, on the audio listener's address; 0 asks for an ephemeral one |
| `--tv-latency-ms <ms>` | 25 (`lowlat::DEFAULTS`) | `L_tv`; refused outside 10..40 or below the plan's floor (24.417 ms at the defaults) |
| `--test-tv-latency-ms <ms>` | off | **tests only**: `L_tv` outside 10..40 (up to 5 s), still never below the floor; a loaded test host does not keep a 25 ms deadline |
| `--fec-k <k>`, `--fec-depth <d>` | 4, 1 | the streams' FEC; `--fec-k 0` is no FEC (the tests' negative control) |
| `--udp-loss <ppm>,<seed>` | off | **tests only**: drop that many datagrams per million, seeded, on both legs (each received from a hub, each sent to a player); never set in a deployment |

With `--slots` the relay is one more thread (`tv-relay`), created with the
rest before the scheduling report: the population is `6 + 2N + M + 1`.

### Speakers: adoption, names and rooms (goal 14)

`docs/decisions/0106-speakers-adopted-named-and-assigned-rooms.md` records the
choices.

**Adoption is automatic and is the handshake's.** The first audio session
under an id pins that id's key (`adopted-endpoints`, the server's file) and,
when the server runs a control plane, makes the speaker's record and logs
`speaker listed id=<id> name="Speaker <tail>" named=0`. Every subscriber is
sent the new state. A server that already holds pins when it starts (one
upgraded from before goal 14) lists each of them, unnamed and in no room. Two
kinds of id are adopted and play but are NOT listed, each logged `speaker not
listed id=<id> reason=<name>`: one that is not a catalog identifier
(`id-not-an-identifier`: no command could name it; the firmware's and the
Linux client's ids are identifiers), and one past the bound of 64 records
(`registry-full`; ASSUMED bound, there because any peer on the LAN can present
a new id and the state message must not grow with each).

**A changed key is never adopted.** A session under an adopted id with
another key is refused `key_changed` and the pin does not move (goal 5:
`an_endpoint_whose_key_changed_is_refused_and_surfaced`,
`an_endpoint_whose_key_changed_is_refused_and_surfaced_by_the_server`). From
goal 14 the refusal is also in the state, `key_changes`, the latest per id,
until the speaker is forgotten or the server restarts.

- **`speaker_name`** names a speaker: `name` is held to the rule a room's is,
  and `named` becomes true.
- **`speaker_room`** assigns a speaker to a room, or to none with `"room":null`
  (a `room` left out is refused: no room is said out loud). The speaker
  becomes a MEMBER of the room (its `endpoints`) and leaves every other. This
  is how an endpoint with no control client gets a room at all: the firmware
  never sends `attach`. From then on the room's stream, `room_volume` and
  `sound` are its session's (`crates/server/src/conductor.rs`, in the session,
  between two chunks), and its PRESENCE is its session's too: it is in the
  room's `present` while a session of it is up and out of it when the last
  one ends. `POST /api/leaving` does not make an assigned speaker absent
  while its session is up.
- **An explicit `speaker_room` wins over the endpoint's own `attach`.** A
  Linux endpoint that attaches itself (`--zone`) and was never assigned works
  exactly as before: `attach` makes it a member, `/api/leaving` makes it
  absent, and its speaker record says `"room":null` (the room there is the
  owner's assignment, not the endpoint's start-up flag). Once it is assigned a
  room, an `attach` naming another room is accepted and attaches it to the
  room it was assigned.
- **`speaker_forget`** removes the record, the speaker's place in any room,
  its link fact, any key change under its id, and the PIN: the next session
  under that id is adopted afresh, unnamed and in no room. It is the owner's
  only way past a changed key (a replaced board, a wiped one). A session of
  the forgotten speaker that is still up is not cut; it plays on in no room
  until it ends. If the pin file cannot be rewritten the command is refused
  whole, naming `speaker`, and nothing is forgotten.
- A speaker that plays in a room's bonded set is not moved or forgotten out
  from under it: both are refused naming `speaker` until the room is
  unbonded.

```json
{"v":2,"t":"speaker_name","speaker":"chorus-0123456789ab","name":"Kitchen left"}
{"v":2,"t":"speaker_room","speaker":"chorus-0123456789ab","room":"kitchen"}
{"v":2,"t":"speaker_room","speaker":"chorus-0123456789ab","room":null}
{"v":2,"t":"speaker_forget","speaker":"chorus-0123456789ab"}
```

The end-to-end test is
`crates/server/tests/adoption.rs::a_new_speaker_is_adopted_named_and_assigned_a_room`:
the real server, the real C endpoint binaries and a Linux client.

### Firmware: staged images and explicit installs (goal 14)

The decisions are `docs/decisions/0110-explicit-firmware-installs.md`; the
owner's page is `docs/firmware-updates.md`; the wire is `docs/protocol.md`,
"Firmware update".

**Nothing installs without an explicit install action (K93, I13).** A server
started with `--firmware-dir <dir>` (it needs `--control-listen`) reads the
directory at start and lists every image staged in it (`<name>.bin` and
`<name>.manifest`: `version`, `board`, `size`, `sha256`), each graded
`verified` or `refused` with a reason by name. A speaker that takes updates
(its session's `capabilities` say `ota`) reports the version, board and slot it
runs, and the state says `update_available` when a verified image for its
board carries another version. That is information: staging, a session coming
up, a reconnect and a server restart send nothing to anybody. Only
`firmware_install` starts a transfer, and nothing about an install survives a
restart (an install in progress when the server stops is not resumed: the
next process tells the speaker to drop it and says `interrupted`).

- **`firmware_install`** names a speaker and a staged image, or says
  `"all": true` for every present speaker of the image's board that is not
  busy and does not run the image's version already. `"force": true` installs
  a version the speaker already runs (a reinstall; written only when true).
  Refused, each naming its field and starting its detail with its name, and
  with nothing sent to anybody: an unknown speaker (`speaker`, the catalog's
  usual words); `unknown-image`, `image-not-verified` (`image`: refused at the
  scan, or changed on disk since); `speaker-absent`, `not-updatable` (it never
  reported a version), `busy` (an install in progress, from `requested` to
  `pending_verify`), `wrong-board`, `already-running` (`speaker`);
  `nothing-to-install` (`all`); `owner-not-at-bench` (`speaker` or `all`: a
  speaker that is not on this host, without the owner's bench variable; see
  below). `all` with any one target refused by the guard is refused whole.
- **`firmware_cancel`** abandons a speaker's install that has not been
  verified yet (`requested` or `receiving`): the server drops the transfer and
  sends the speaker the cancel. Refused `nothing-to-cancel` otherwise; an
  image already verified is the speaker's to boot.
- **`firmware_rescan`** reads the directory again and grades every image.
  Refused `no-firmware-dir` (field `t`) on a server without one.

```json
{"v":2,"t":"firmware_install","speaker":"chorus-0123456789ab","image":"brick-2-0-0"}
{"v":2,"t":"firmware_install","all":true,"image":"brick-2-0-0"}
{"v":2,"t":"firmware_install","speaker":"chorus-0123456789ab","image":"brick-2-0-0","force":true}
{"v":2,"t":"firmware_cancel","speaker":"chorus-0123456789ab"}
{"v":2,"t":"firmware_rescan"}
```

**The owner-at-bench guard (program section 0.7).** A transfer to a speaker
whose session's peer address is not loopback writes to a real device, which is
the owner's action: it is refused `owner-not-at-bench` unless the server's
environment holds `CHORUS_OWNER_AT_BENCH` set to `1`. Nothing in this
repository sets it; `docs/firmware-updates.md` says how the owner's deploy
does. Tests and the emulator run reach the server on loopback.

A speaker's `firmware.state` is what it is doing (`idle`, `requested`,
`receiving`, `verified`, `pending_verify`) or how the last install this
server process saw ended (`confirmed`, `rolled_back`, `refused`,
`interrupted`, `cancelled`); an outcome stays until the next install or a
restart, so a rollback is still shown after the speaker's next `idle`.

The end-to-end tests are `crates/server/tests/firmware_install.rs`, the first
of them `nothing_installs_until_the_explicit_install_action`.

### Stored sources and input labels (goal 17)

```json
{"v":2,"t":"source_store","id":"morning-radio","kind":"url","value":"https://radio.example/stream.mp3","name":"Morning radio"}
{"v":2,"t":"source_store","id":"wake-playlist","kind":"spotify","value":"spotify:playlist:37i9dQZF1DXexample0000","name":"Wake up"}
{"v":2,"t":"source_forget","id":"morning-radio"}
{"v":2,"t":"input_label","input":"endpoint-c/line-1","name":"Living room streamer","role":"streamer"}
```

A **stored source** is the one way a URL is KEPT by the server through this
catalog (goal 18's `announce`, below, plays a URL from a configured origin
and keeps nothing), and only an alarm plays it (`"source":"stored:<id>"` in `alarm_set`;
`docs/inputs.md` says how it is played and what happens when it cannot be).
`kind` `url`: `value` is `http://` or `https://` followed by a host, holds no
control character, space, quote or backslash, and is at most 2048 bytes
(ASSUMED bound, the art URL's). The scheme and the shape are checked when it
is stored, field `value`; whether the server may fetch it is the fetch
policy's to say when the alarm rings (`docs/streams.md`), not when it is
stored, because the answer depends on where the name resolves that morning.
`kind` `spotify`: `value` is `spotify:<track|album|playlist|episode>:<id>`,
the id 1 to 64 ASCII letters and digits; it is stored and validated here and
played by a Soloist receiver, which ships switched off. `name` is a display
name. At most 32 are held. An `alarm_set` naming a stored source that does
not exist is refused, field `source`, listing the ones that do; a
`source_forget` of one an alarm plays is refused, field `id`, naming the
alarms. The state lists them, values and all, as `stored_sources`: whoever
can read the state can read a stored URL, so a URL with a credential in it
is a credential every subscriber sees.

An **input label** is a person's name for an input and what is wired to it.
The role `line-in` is any input; `streamer` says a bought network streamer
(the box that carries the licensed receivers chorus does not implement, K60)
is wired to it. A `streamer` input plays into its endpoint's own room when
its signal appears, with no `autoplay` rule (an `autoplay` rule for the
input, enabled or not, is the person's word and wins); the groups playing it
show the label (`now_playing`, `via` `streamer`); and it is shared to any
group like any line-in. There is no microphone role (brief section 4.8). A
label is configuration: it is kept whether or not the input is offered now,
and at most 32 are held. The state lists them as `input_labels`.

### Spotify receivers: the `soloist:` source, playback and restart (goal 17)

```json
{"v":2,"t":"playback","target":"kitchen","action":"pause"}
{"v":2,"t":"soloist_restart"}
```

A server started with `--soloist-receivers` (`docs/soloist.md`) gives every
room, saved group and live group a Spotify Connect device, and a group whose
device is playing has the source `soloist:r<i>`, receiver `i`'s audio. It is
a source like `player:<id>`: it plays in one group at a time, a `take` moves
it and never copies it, and the group carries a now-playing record with `via`
`spotify`. **No command names it.** A receiver plays where the Spotify app
plays it, so a `take` (or an alarm) with a `soloist:` source is refused by
name, and the only thing that gives a group that source is the server's own
receiver manager, when the device starts playing (take the room, below, done
by the server).

`playback` is the catalog's one transport command. It existed in no form
before goal 17 (a UPnP cast is controlled by its control point, not by this
catalog), so it is new, and it reaches a Spotify receiver only: the target's
formed group must be playing one. `pause`, `resume`, `next` and `previous`
become Soloist's `pause`, `play`, `skip_next` and `skip_prev`. The state does
not change when the command is applied; it changes when Soloist reports what
it did (the now-playing record's `state`).

`soloist_restart` is what the owner sends, as `chorusctl soloist restart`,
after replacing an expired Soloist binary.

### Announcements: the `announce` command (goal 18)

```json
{"v":2,"t":"announce","target":"kitchen","url":"http://ha.example:8123/api/tts_proxy/abc.mp3"}
{"v":2,"t":"announce","target":"kitchen","url":"http://ha.example:8123/api/tts_proxy/abc.mp3","volume":0.300}
```

Brief section 4.8 names three paths by which the server may be made to fetch
a URL: UPnP renders, "HA's media and TTS URLs from HA's own address", and
stored alarm stream URLs. `announce` is the second, and it is held to those
words: **only a URL whose scheme, host and port are those of an origin the
server was started with is played** (`--announce-origin
<scheme://host[:port]>`, repeatable; the home automation's own address). So
there is still no command that plays an ARBITRARY URL
(`docs/decisions/0124-the-media-player-engine.md`), and a server started
with no origin refuses every `announce` by name.

| field | notes |
|---|---|
| `target` | a room, a saved group or a formed group, as `take` names one |
| `url` | `http://` or `https://`, under the shape rules a stored `url` source has (a host, no control character, space, quote or backslash, at most 2048 bytes), from a configured origin |
| `volume` | optional: the volume the target's rooms play the clip at, CLAMPED to each room's effective limit like every volume path; the rooms get their own back afterwards. Absent: the rooms keep their volumes |

The answer is `200` with the resulting state (the group's source is the
player, its now-playing record says `via` `announce`), or `400` with an
`error` naming the field:

| field | when |
|---|---|
| `target` | no room or group has that id; or an alarm is ringing in a room that would hear the announcement (an announcement does not interrupt a ringing alarm) |
| `url` | not `http`/`https` or not the shape above (the decoder); no origin configured (`no-announce-origin: ...`); an origin that is not on the list, or a URL with userinfo or a port that is not one (the refusal lists the configured origins) |
| `volume` | outside `0.000` to `1.000` |
| `t` | the server runs no player (`no-players: ...`, a server started without `--players`), or none is free (`no-free-player: ...`) |
| `source` | no stream slot is free for the group (the slots' own refusal) |

An origin is compared whole: the scheme, the host as text (ASCII
case-insensitive; a name is never resolved to compare it) and the port, the
scheme's default (80, 443) when none is written. `http://ha.example:8123` and
`http://ha.example` are two origins. The fetch itself goes through the fetch
policy (`docs/streams.md`) and is **held to the configured origins at every
connection it makes**: the fetcher follows redirects (up to 5), and a redirect
that leaves the origin fails the fetch (`refused: origin ...`) without
connecting to where it points.

**What it does is interrupt and restore, and nothing more** (goal 20 builds
the ducking mixer, K31, and replaces the playback here, not the command):

1. The clip plays through a held player session, as a ringing alarm's stored
   URL does (`docs/inputs.md`): the server takes a free player, the target's
   group plays `player:p<i>`, and the state shows it (`now_playing`, title
   `Announcement`, `via` `announce`). A room plays in **the group it is in**,
   so everybody in that group hears it and nobody is regrouped; a saved group
   that is not active is taken first (K78), as any play on a saved group
   does, and stays formed afterwards.
2. With `volume`, every room of the group is set to it, clamped.
3. When the clip ends, fails (a 404, a refused redirect, an undecodable
   file), or has played for 10 minutes (ASSUMED bound: a URL that turns out
   to be an endless stream must not hold a room), the group goes back to the
   source it had, the player is given back, and each room whose volume is
   still the one the announcement set goes back to the volume it had. A
   failed fetch restores at once: the command was answered `200` (the fetch
   runs on the player's thread), and the server's log has the reason
   (`announce owner=announce:<n> ended group=<g> restored=<source>
   failure="<words>"`).

The limits, plainly:

- **No mixing, no ducking.** The music stops for the clip; it is not turned
  down under it.
- **Interrupted, not paused.** The stream, a line-in and a chime come back
  as they are when the clip ends (they are live or generated, so there is no
  position to keep). A UPnP cast and a Spotify receiver do NOT come back: a
  player is given back the moment its group stops playing it and a receiver
  is paused by its manager, so a group that was playing either plays `none`
  after the announcement and is started again from the app that drove it.
- **One player per announcement.** A server whose players are all in use (a
  cast in another room, an alarm's stream) refuses the announcement
  (`no-free-player`). A group that is casting needs a second player for the
  clip.
- **A room target in a multi-room group is heard by the whole group.**
- **An announcement during another**, in the same group, replaces it on the
  same player; what the group goes back to is what it played before the
  first. In another group it needs a player of its own.
- **An alarm that rings during an announcement** takes its rooms as it always
  does and the announcement ends; when the alarm ends the rooms go back to
  what they played, and the volumes they had, BEFORE the announcement. **An
  announcement for a room an alarm is ringing in** is refused, field
  `target`.
- **A person's command during the clip wins.** A `take`, a `join` or a group
  change that gives the group something else ends the announcement and is
  not undone; a volume a person set during the clip is kept.
- A volume the announcement set is persisted like any volume, so a server
  that stops in the middle of a clip starts again at the clip's volume.
- The schedule runtime is not told of an `announce` as it is of a person's
  command: it ends no autoplay and detaches no room.

`crates/server/tests/announce.rs` runs it on the real binary;
`docs/decisions/0136-a-server-identity-and-an-announce-command.md` records
the decision.

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

(goal 17) A server started with `--soloist-receivers` adds one member after
everything else, `soloist`, and a server without the flag writes none, which
is why no earlier state vector moved (`fixtures/control/v2/state-soloist.json`
pins it):

```json
"soloist":{"receivers":[{"id":"r0","state":"running","target":"room:kitchen","name":"kitchen"},{"id":"r1","state":"absent","target":"","name":""}],"build":{"version":"Soloist 1.3.8.96, build 20260930, Linux/aarch64","expires_in_days":10},"warning":"Soloist build expires in 10 days","exhausted":["room:den"]}
```

| field | notes |
|---|---|
| `receivers[]` | every receiver, in order: `id`; `state` (`absent` while the server holds no connection to its supervisor, else the supervisor's word: `idle`, `starting`, `running`, `expired`, `failed`, `no-binary`); the `target` it is assigned to (`room:<id>`, `group:<id>`, `live:<a>+<b>`) and the Spotify Connect device `name`, both empty for none |
| `build` | written once a supervisor has reported a binary: the first line of `soloist --version`, and `expires_in_days` (whole days, negative once expired) when that line names a build time |
| `warning` | written only while it holds: `Soloist build expires in N days` from 14 days before, `Soloist build expired` after |
| `exhausted` | written only when not empty: the targets the pool had no receiver for |

It is a fact about now, set by the server and never persisted.

A server with no zone configured serves `{"v":1,"t":"state","serial":0,
"zones":[]}`. That is a state and not an error, and
`fixtures/control/state-empty.json` pins it, so "no zones yet" is a shape the
catalog declares rather than something a client infers from a missing field.

That is the v1 shape, served on `?v=1`. **The v2 state** is the same message
with more fields, in this order (`fixtures/control/v2/state-rich.json` pins one
of each, `fixtures/control/v2/state-empty.json` the empty house):

| field | notes |
|---|---|
| `zones[]` | v1's fields, then `transport` (`wired` or `wireless`, as declared), `limit`, `effective_limit`, `quiet` (`[{"days","start","end","limit","active"}]`; `active` says the clock is inside the window, whether or not the room's quiet hours are switched on), `quiet_enabled` (`true` or `false`: whether an active window caps the room; `true` unless `quiet_hours_enabled` switched it off), `bond` (`[{"endpoint","role"}]`, `[]` for none), `ramp` (a running ramp's target, or `null`), and (goal 12) `sound` (`{"bass","treble","loudness","night","speech"}`), `bass_management` (`{"crossover_hz","sub_level_db","sub_polarity","active"}`) and `room_eq` (`{"enabled","filters"}`) |
| `zones[].source`, `zones[].now_playing` | (goal 16) **written only while the room's group has a now-playing record**, last in the room's object, in that order: the group's `source` (a `player:<id>`) and the record, below |
| `groups[]` | every formed group, in the order its first room was configured: `id`, `kind`, `zones`, `volume` (the group volume), `source`, `audio` |
| `groups[].now_playing` | (goal 16) **written only while the group has a now-playing record**, after `audio`: `title`, `artist`, `album`, `art_url` (each a string, or `null` where not known), `duration_ms` (a whole number, or `null` for a live stream or an unknown length), `state` (`playing`, `paused` or `buffering`), `via` (an identifier: `upnp` for a UPnP AV control point). Never persisted |
| `saved_groups[]` | every saved definition, sorted by id, active or not: `id`, `name`, `zones`, `active` |
| `endpoints[]` | every endpoint any room has or that has reported a link, sorted: `id`, `link` |
| `alarms[]` | sorted by id: `alarm_set`'s fields from `alarm` on, then `ringing` |
| `sleep[]` | the sleep timers asked for, sorted by target: `target`, `minutes` |
| `autoplay[]` | sorted by input: `autoplay`'s fields |
| `inputs[]` | the line-ins offered now, as `<endpoint>/<input>`, sorted |
| `stored_sources[]` | (goal 17) **written only when there is at least one**, after `inputs`: every stored source, sorted by id: `id`, `kind`, `value`, `name` |
| `input_labels[]` | (goal 17) **written only when there is at least one**, after `stored_sources`: every label, sorted by input: `input`, `name`, `role` |
| `speakers[]` | (goal 14) **written only when there is at least one**: every adopted speaker, sorted by id: `id`, `name`, `named`, `room` (the assigned room, or `null`), then what is true now: `present` (a session of it is up), `software` (its latest `hello`'s, `""` before one), `link` (what it reported with `attach`, as in `endpoints[]`; `unknown` for an endpoint with no control client), `key` (the fingerprint of its pinned key), `roles` (its latest `hello`'s, by name) |
| `key_changes[]` | (goal 14) **written only when there is at least one**: every handshake refused for a changed key since the server started, the latest per id, sorted by id: `id`, `pinned`, `offered` (fingerprints) |
| `speakers[].firmware` | (goal 14) **written only once the speaker has reported** (its session declared `ota` and sent a `firmware_status`), after `roles`: `version`, `board`, `slot` (0, 1 or `null`) it runs, `state`, `reason` (`none` or the reason by name), `update_available` (a verified staged image for its board with another version; derived, never stored), and the install the state is about: `image` (the staged name, `null` when none or when this server did not start it), `image_version`, `received` and `size` (bytes; the progress). Never persisted |
| `firmware` | (goal 14) **written only by a server with `--firmware-dir`**, last: `{"images":[...]}`, every staged image sorted by name: `name`, `version`, `board`, `size`, `sha256`, `verdict` (`verified` or `refused`), and `reason` for a refused one. Never persisted (the directory is) |

`speakers` and `key_changes` come after every member the v2 state already had
and are absent, not empty, on a server that has adopted nothing, so
`state-empty.json` and `state-rich.json` are the bytes they were;
`fixtures/control/v2/state-speakers.json` pins both, and
`fixtures/control/v2/state-firmware.json` the firmware members. The same rule
holds for what is playing (goal 16): a room and a group with no now-playing
record carry neither `now_playing` nor, on the room, `source`, so every vector
committed before it is the bytes it was, and
`fixtures/control/v2/state-playing.json` pins a house where two groups have a
record, one plays a player nothing was said about, and one plays the stream.
And for goal 17's two members, which come after `inputs` and before
`speakers`: `fixtures/control/v2/state-inputs.json` pins a house with two
stored sources, an alarm that plays one, and a labelled streamer two groups
play at once, each showing its label.

**The now-playing record's bounds.** The server holds what it is told to
these, and the state never carries more: a title, artist or album has every
control character replaced by a space, is trimmed, is cut to 256 bytes of
UTF-8 at a character boundary (ASSUMED bound), and is `null` when nothing is
left; `art_url` is `null` unless it starts `http://` or `https://`, holds no
space or control character and is at most 2048 bytes (a longer URL is dropped,
never cut); `duration_ms` is `null` above 9007199254740991 (2^53 - 1, the
largest whole number every JSON reader holds exactly, RFC 8259 section 6);
`via` is an identifier. The record is on the room as well as on the group
because a consumer of one room reads the room's object: the MQTT room topic
is that object, byte for byte (`docs/mqtt.md`). A room's controllers are shown
`paused` while its group's record says `paused`, and `playing` otherwise
(`docs/protocol.md`, "0x33 controller state"). A reader takes the
members it knows by name and ignores the rest (the page and the Linux client
do); a later goal's per-speaker facts are appended to a speaker's object
after `roles`.

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

The speaker refusals name `speaker`, `name` or `room`, and say what there is
(`error-speaker-*`):

```json
{"v":2,"t":"error","field":"speaker","detail":"there is no speaker 'chorus-ffffffffffff'; the speakers adopted on this server are chorus-0123456789ab"}
{"v":2,"t":"error","field":"speaker","detail":"speaker 'chorus-0123456789ab' plays in room 'living''s bonded set; unbond room 'living' first"}
```

## How the messages travel

The catalog above is the contract. The transport is HTTP on the address
`--control-listen` names, and it is deliberately ordinary:

| route | what it does |
|---|---|
| `GET /` | the control page |
| `GET /chorus.css`, `GET /chorus.js` | what the page loads |
| `GET /api/state` | the state message, once (v2; `?v=1` for the v1 shape) |
| `GET /api/server` | (goal 18) who this server is, once: the `server` message below. Not a state: nothing changes it while the server runs and no state message carries it |
| `GET /api/events` | a `text/event-stream`, one `data: <state message>` per change, starting with the state as it stands (v2; `?v=1` for the v1 shape, rendered from the state as it stands when each change reaches the stream) |
| `GET /api/controller-events` | a `text/event-stream`, one `data: <controller_event message>` per controller command the server accepts (a button press on a speaker), from the moment the stream is opened. It starts with no message: a press is not a state, and one made before the stream was opened is never sent to it. Catalog version 2's message alone; `?v=1` changes nothing |
| `GET /api/report` | one line of plain text: commands applied, commands refused, connections and streams turned away, the fanout's ceiling and drops, the event writer's streams, and the controller-event subscribers and their drops |
| `GET /metrics` | the Prometheus exporter (goal 15): per-speaker telemetry in text exposition format 0.0.4. Read-only, served by a control worker like any other request; `docs/telemetry.md` lists every series |
| `POST /api/command` | the body is one control message, sent as `Content-Type: application/json`. `200` with the resulting state (v2, the bytes every subscriber is sent), `400` with an `error` (at the message's version), or `426` with a `refused`; `415` or `403` under the rules below |
| `POST /api/leaving` | the body is an endpoint identifier, which stops being `present`. The same two rules as a command |

`GET /api/server` answers `200`, `application/json`, with one message in the
canonical encoding (`fixtures/control/v2/server.json`,
`server-no-origin.json`):

```json
{"v":2,"t":"server","id":"chorus-server-0123456789abcdef","software":"chorus-server 0.1.0","catalogs":[1,2],"announce_origins":["http://ha.example:8123"]}
```

| field | notes |
|---|---|
| `id` | a stable identifier of this server, 1 to 64 lower-case ASCII letters, digits and hyphens: `chorus-server-` and the 16 hexadecimal digits of the fingerprint of the server's public key (30 characters). It is the same for as long as the key is: across restarts of a server with an identity directory (`--identity-dir`, or `--state-file`'s directory), and new at every start of a `--ephemeral-identity` server, which is a throwaway run. The control service's TXT record carries the same value as `id=` ("Discovery") |
| `software` | the server's version string, the one its audio `hello` carries |
| `catalogs` | every catalog version this build implements |
| `announce_origins` | the `--announce-origin` list, each in its one spelling (`scheme://host`, then `:port` unless it is the scheme's default), in the order given; `[]` for none, and then every `announce` is refused |

A browser opens `/api/events` with `EventSource` and a shell script opens it
with a socket and a `GET` line, so **the UI and the verification scripts are the
same subscriber**. That is deliberate: a check that exercised a second, private
subscriber protocol would not be checking the thing the browser uses.

`GET /api/controller-events` is the same subscriber protocol with a different
payload: what a speaker's button asked for, as an event
(`docs/decisions/0151-controller-events-on-the-http-control-plane.md`). The
response is `200`, `text/event-stream`, then one comment line
(`: controller events`) and nothing else until a press. Every controller
command the server accepts (`docs/protocol.md`, "controller command") is then
one `data:` line holding one message in the canonical encoding
(`fixtures/control/v2/controller_event.json`,
`controller_event-transport.json`):

```json
{"v":2,"t":"controller_event","endpoint":"endpoint-a","zone":"kitchen","command":"volume_step","value":-5,"target":"","outcome":"applied"}
```

| field | notes |
|---|---|
| `endpoint` | the endpoint whose button it was: its session's authenticated id |
| `zone` | the room that endpoint plays in, which the command acted on |
| `command` | the protocol's name for the command: `play`, `pause`, `toggle`, `next`, `previous`, `volume_set`, `volume_step`, `mute_set`, `join`, `leave` |
| `value` | its argument, an integer; `0` where the command takes none |
| `target` | the group for `join`; `""` otherwise |
| `outcome` | `applied` when the command changed the room; `waits-for-an-input` for a transport command, which acts on an input and changes no room |

The members after `t` are the MQTT event's, in its order, with the same values
(`docs/mqtt.md`, "The topics"); the MQTT payload is unchanged and carries no
`v` or `t`. An event is not a state change and rides in no state message: a
press that changes a room is one `controller_event` on this route AND one state
message on `/api/events`, the event fanned out first; a transport command is
the event alone. A command the server refused (an endpoint attached to no room, a `join`
whose target is not a group name) is no event. Nothing is kept: a press
accepted while nobody is subscribed is sent to nobody, and a stream opened
afterwards starts empty. It is a message the server sends and never one it
accepts: `POST /api/command` refuses it as an unknown message type.

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

A `GET /api/controller-events` stream is held by the same writer under the same
rules: its worker writes the headers and the opening comment line and hands the
socket over, it counts against the same `--event-streams` ceiling as the state
streams (the two kinds together never hold more sockets than that), it is sent
the same keepalive comment, and it is dropped after 5 s without write progress
and counted in the same `stalled_dropped`. It differs in what it is fed, a
second fanout that carries `controller_event` messages, and in its opening:
nothing (`crates/server/tests/controller_events.rs`).

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

A subscriber of `GET /api/controller-events` is held to the same bound by the
same code, on a queue of its own: at most 32 `controller_event` messages wait
for it (a few hundred bytes each, so on the order of 10 kB per stalled subscriber), and one
that reaches the ceiling is **dropped**, its stream closed, never the event
skipped for it: a subscriber that stays attached has been sent every press
accepted since it attached, in order. `GET /api/report` counts them apart from
the state subscribers, as `press_subscribers` (attached now),
`press_dropped_subscribers` (dropped at the ceiling) and
`press_dropped_events` (events a dropped or departed subscriber never
received). There is no replay on either side of a drop: a subscriber that was
dropped and opens a new stream is sent the presses accepted from then on, and
the ones in between are gone. A press is never delayed or refused because a
subscriber is slow: the fanout is one non-blocking `try_send` per subscriber,
on the session's reader thread, before the endpoint is answered.

## The thread population

With `--control-listen`, the server runs one control acceptor, one worker per
`--control-workers` slot, the event writer (above) and the **conductor**
(below), all created before the scheduling report is taken and none created
afterwards however many subscribers, sessions or groups come and go. The whole
process is `6 + 2N + M` threads, plus one for `--advertise`, and it does not
depend on `--slots`: every stream slot is cut by the one audio thread. Nor
on `--firmware-dir` (goal 14): an image travels in the speaker's own session,
queued by the control worker that applies `firmware_install` (the offer) and
by that session's own reader (the chunks). With `--mqtt-broker` (goal 15,
`docs/mqtt.md`) there is one thread more, the `mqtt-publisher`, created with
the rest before the scheduling report: `6 + 2N + M + 1`. Without the flag it
does not exist. Everything that touches the broker happens on it, so a broker
that is down or silent holds no worker, no reader and not the conductor.
With `--players P` (goal 16; it needs `--slots` of at least 1, and P is at
most 16) there are P threads more, `player-0` to `player-<P-1>`, created with
the rest before the scheduling report whether or not anything ever plays:
`6 + 2N + M + 1 + P`, the one being the TV relay that `--slots` brings.
Without the flag there is none. Each is the one writer of its player port; a
stream starting, ending or changing makes no thread, because a thread made
when a stream starts would be one the report never saw.
With `--soloist-receivers R` (goal 17, `docs/soloist.md`; it needs the control
plane and `--slots`, and R is at most 32) there are `R + 1` threads more,
`soloist-reader-0` to `soloist-reader-<R-1>` and `soloist-manager`, created
with the rest before the scheduling report whether or not any receiver
container is running. Without the flag, or with `--soloist-receivers 0`,
there is none. A receiver being assigned, a Spotify session starting and a
group taking a receiver make no thread.
With `--upnp` (goal 16, `docs/upnp.md`; it needs the control plane, `--slots`
and `--players` of at least 1 each, and a persisted identity) there are
`4 + W` threads more, W being `--upnp-workers` (default 4, at most 16):
`upnp-ssdp`, `upnp-acceptor`, `upnp-events`, `upnp-manager` and
`upnp-worker-0` to `upnp-worker-<W-1>`, created with the rest before the
scheduling report: `6 + 2N + M + 1 + P + 4 + W`. Without the flag there is
none. A control point that searches, reads a description, calls an action or
subscribes makes no thread, and neither does a renderer appearing or
vanishing: a connection that arrives with every worker busy is answered 503,
and every event to every subscriber leaves from `upnp-events`. The renderers
have their own HTTP port (`--upnp-listen`, default 4030) and their own
workers, so a control point never holds a control worker.

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
  <input>` is that endpoint's input (below, "The schedule runtime");
  `player:p<i>` (goal 16) is that player's port (below, "Players in the
  slots"); `none` is no slot at all. In this shape a source that ends (a file) is reopened, and no
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
  cannot play (a line-in not offered, a chime that does not exist, `none`, a
  `player:<id>`, a stored source that cannot be played) plays the `bell`
  chime instead, with the reason in the log (`schedule alarm=<id>
  fallback=chime reason=<why>`). A line-in another group already plays is
  played here too (goal 17: there is no `input-busy` any more). A stored
  source (`stored:<id>`, goal 17) fires in silence while the server starts
  what plays it, then the group plays that (`docs/inputs.md`, "An alarm with
  a stored source"). It ends after `duration_min`, on `alarm_stop` or
  `alarm_delete`, or when a person changes one of its rooms; then it fades
  out and the rooms go back to what they were (their group, volume, mute and
  source).
- **Sleep timers** fade the target to 0 over the last 30 s, then set its
  source to `none` and put the volumes back, silently, for next time. A person
  turning a fading room's volume cancels the timer.
- **Quiet hours** follow the civil clock: a window starting pulls a room's
  volume down to its cap (and holds an alarm's rise under it); a window ending
  raises nothing. A room whose quiet hours are switched off
  (`quiet_hours_enabled`) is not pulled down by a window starting and its
  ramps rise to its `limit`; switching them back on inside a window pulls it
  down then, by the same ramp-then-limit pair of reports.
- **Line-in autoplay**: an input whose signal arrives with an enabled rule
  takes the rule's target and plays the input (goal 17: whether or not
  another group already plays it; only a target that already plays it,
  exactly as the take would leave it, is left alone, with nothing held and
  nothing to restore; and an input labelled `streamer` with no rule at all
  plays into its endpoint's own room); when the signal goes it is held
  30 s (on top of the endpoint's own 2 s), then stopped and the target
  restored; the signal coming back within the hold cancels the stop. A
  person's command that changes what a held room plays or where it is
  (`group`, `ungroup`, `join`, `take`) lets that room go (`schedule autoplay
  input=<id> zone=<room> detached by a person's command`); a volume or a mute
  (`volume`, `volume_step`, `group_volume`, `group_volume_step`, `mute`,
  including a TV remote's keys through the hub's CEC role) does not, so the
  end of the autoplay still restores the room, to the volume and mute it had
  before the autoplay took it (goal 13, ADR 0092). An alarm is unchanged: any
  person's command naming its room ends it.
- **TV autoplay** (goal 13, K81): a TV input (`optical`, `hdmi_arc`) is an
  input like any other, so the TV coming on is a signal and its rule's `take`
  takes the target room out of whatever group it was in. The TV going to
  standby is a `source_offer` with `signal` false and `reason` standby
  (`docs/protocol.md`, 0x36), which the server hands the runtime as a standby
  (`InputEvent::Standby`; an offer from any other kind of input with that
  reason is only a signal gone): an autoplay of it whose rule says
  `stop_on_standby` stops NOW, no hold, and restores its rooms
  (`schedule autoplay input=<id> stopped reason=standby restored=<rooms>`);
  one whose rule says otherwise holds as for a signal gone.

Every default above is ASSUMED and listed in ADR 0076 and ADR 0079.

**Line-ins in the slots.** A line-in is the source role's input (ADR 0066) on
an endpoint's own session. The server starts it (`source_control` start, PCM,
in that session) while some group plays it and stops it when none does; the
input's `stream_format` must be the server's own format or it is stopped and
refused by name (`line-in refused reason=format-mismatch`). Its chunks go into
one of S fixed ports, each holding one second. (goal 17) A port is one
INPUT's, however many groups play it: every slot whose group plays the input
cuts the same port's chunk at each tick, so the groups hear the same samples
on the same grid, and since every group playing an input holds a slot, at
most S inputs stream at once and S ports are enough; the audio thread plays a port through the latency-growth plan and the
cubic resampler (`crates/sync/src/latency_grow.rs`, ADR 0071) once it holds
two chunks. The chunks go out on the slots' ONE grid (the same sequence and
timestamp as every other slot), so moving into or out of a line-in's group is
no timestamp jump; what the plan moves is which source frames each chunk
carries. A port has ONE plan and so one latency target (goal 17): the
largest any group listening to it needs. While the only listener is the
source endpoint's own room, alone in its group, the line-in plays at L_local
(30 ms, ASSUMED); once another room joins that group, or another group
plays the input, the plan grows the offset to the largest listening group's
latency (180 ms wired, the wireless tier's 500 ms) by a bounded, smooth time
stretch (500 ppm at most), with no frame dropped, repeated or zero-inserted
for the room already playing, and shrinks it back the same way when the room
is the sole listener again (K94). A target asked for while a transition runs
waits for it to end (ADR 0071), so a listener that comes and goes within a
transition leaves the first room on the latency that transition reaches
before it comes back down. The server says each change of target or of
listeners (`line-in latency port=<p> listeners=<n> groups=<ids>
target_ms=<ms>`). A tick that finds too little
of the input plays silence and the plan waits (counted, `underruns=` in the
`line-in` lines). The latency is the stamp offset on the wire; today's
endpoints add their own playout latency on top of every stamp, so a line-in
is heard at L_local plus that until an endpoint mode for local line-ins
exists (ADR 0079's follow-ups).

**Players in the slots (goal 16).** `--players N` gives the server N network
media players, `p0` to `p<N-1>`, each a **player port**
(`crates/server/src/playerport.rs`): a ring holding one second (ASSUMED) of
decoded audio at the server's own sample rate and channel count, allocated at
start. A player's thread decodes, converts to that rate and those channels,
and writes; nothing it writes is ever dropped (a full ring accepts what fits
and the thread waits). At each tick the audio thread takes exactly one chunk
from the port of every player some slot plays, in the order written, and
plays silence for whatever the ring lacks: a port that is held (paused, or
filling) or empty plays silence, and the last frames of the media go out
padded with silence, not held back. The chunks go out on the slots' one grid
like the configured stream's, so there is no latency plan and the group's
tier latency applies as it does to `stream`. The port counts the frames
written and the frames taken since the last flush, which is how the thread
knows its position and the moment a second track, written straight after the
first, starts to go out (gapless: the audio thread never sees a boundary). An
alarm whose source is a `player:<id>` rings the `bell` chime (reason
`player-source`): an alarm has no media to hand a player. A room that was
playing a player when an alarm or an autoplay took it gets the player back
when that ends, unless another group took the player meanwhile: then the room
plays `none` (`schedule restore group=<id> source=none reason=player-busy`).

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
described by one record. The TXT record carries `v=<catalog version>` and
then, on the audio service, `ctl=<control port>`, and (goal 18) on the
control service, `id=<the server's id>`: the `id` of `GET /api/server`, so a
controller that already knows a server (Home Assistant's zeroconf discovery)
recognises it when its address changes. `fixtures/discovery/advertisement-control`
pins the control service's packet and what a resolver makes of it, for the
Rust crate and the endpoint's C resolver alike. The audio service's record is
unchanged.

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

An endpoint's telemetry is not in the catalog and not in the state message: it
is served on `GET /metrics` (`docs/telemetry.md`), so a report a second per
speaker never becomes a state change. Also absent, and belonging to later
phases: presets. Catalog v2 names a group's source and the inputs offered, and
(goal 16) the state says what a player source is playing; choosing what a
player plays (a stream URL, a service) is not a control message: it is said to
the player by what drives it (a UPnP AV control point, `docs/upnp.md` once the
renderer lands). (goal 17) `source_store` stores a URL and plays nothing: the
one thing that plays a stored source is an alarm ringing, and a `take` naming
one is refused. (goal 18) `announce` is the one message that carries a
URL to be played now, and only from an origin the server was started with
(the home automation's own address): it is not a way to play an arbitrary
URL, and a server with no `--announce-origin` plays none. What a Spotify receiver plays is chosen in the Spotify app;
the catalog can pause, resume and skip it (`playback`) and nothing else: no
message plays a Spotify URI in a room, searches or queues.

No firmware image travels over this channel (goal 14): a request is at most
16 KiB and an image is megabytes. Images are staged as files in
`--firmware-dir`, and the bytes go to a speaker inside its own audio session.

## What survives a restart (state-file format 7)

The server persists, in `--state-file`, everything a person configured: each
room's name, group, volume, mute, endpoints, `limit`, quiet-hours windows,
bonded set, (format 3, goal 12) its sound, bass management and correction
filters, and (format 4, goal 13) its `tv_upmix` and `av_trim_ms`; each
endpoint's link; saved groups; alarms; autoplay rules (with, from format 4,
`stop_on_standby` and `low_latency`); and (format 5, goal 14) each adopted
speaker's `name`, `named` and `room`, in a `[speaker <id>]` section; and
(format 6, goal 17) each stored source (`[stored-source <id>]`: `kind`,
`value`, `name`) and each input label (`[input-label <endpoint>/<input>]`:
`name`, `role`); and (format 7) whether each room's quiet hours are switched
on (`quiet_enabled`, 0 or 1, in its `[zone]` section, required in a format 7
file). It does
not persist what is a fact about now: which endpoints are present, which quiet
window is active, which alarm is ringing, a running ramp, what a group is
playing (its source and, goal 16, its now-playing record), a sleep timer,
which inputs are offered, or a speaker's presence,
software, roles, key fingerprint and refused key changes (the pins themselves
are the server's `adopted-endpoints`, beside its key), or anything about
firmware: the staged images (the directory is read again at start) and a
speaker's version, state and install, so an install in progress is never
resumed by a restart (goal 14). A format 1 file (every build
before catalog v2) loads unchanged with the v2 defaults, and a format 2 file
(goal 11's builds) with the sound defaults, and a format 3 file (goal 12's
builds) with no trim, the upmix off and every rule's TV fields true, and a
format 4 file (goal 13's builds) with no speaker record (the server lists
every id it has pinned when it starts), and a format 5 file (goal 14 to 16's
builds) with no stored source and no input label, and a format 6 file (goal
17's builds and later, before `quiet_hours_enabled`) with every room's quiet
hours enabled, which is what its windows meant then; the next write is format 7. A write goes to a temporary that is `fsync`ed, renamed over the file, and the
directory is `fsync`ed, and a render that would not read back as the same state
is never installed (`crates/control/src/persist.rs`).
