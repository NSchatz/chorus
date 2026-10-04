# 0136: the server says who it is on `GET /api/server` and as `id=` in its control service's TXT record, an id derived from its public key; `announce` plays a clip only from an origin the server was started with, the fetch held to that origin through every redirect, by interrupting the target's group through a held player session and restoring what it played and the volumes its rooms had; the catalog version stays 2

- Status: accepted (goal 18, 2026-10-04)
- Decided by: the goal (the Home Assistant integration; brief section 4.8, K31, K78) inside the
  coordinator's contract for track S, branch `chorus-g18/server-seams`; every number below not
  cited is ASSUMED
- Implemented in: `crates/control/src/catalog.rs` (`Command::Announce`, `ServerInfo`,
  `is_server_id`), `rooms.rs` (`Origin`), `zones.rs` (`announce_check`, `announce_group`,
  `announce_begin`, `Announced`); `crates/fetch/src/policy.rs` (`Policy::origins`, `Origin`,
  `check_origin`), `http.rs` (asked at every connection);
  `crates/server/src/announce.rs` (new), `control.rs` (the route, the command's path,
  `announce_begin`, `announce_end`), `conductor.rs` (`settle_announcements`),
  `schedule_runtime.rs` (`Runtime::announcement_over`), `playersessions.rs` (`held_epoch`, a
  request's origins), `mediaplayer.rs` (`Action::LoadWithin`), `config.rs` and `main.rs`
  (`--announce-origin`, the identity loaded before the advertiser); `fixtures/control/v2/`
  (nine new vectors), `fixtures/discovery/advertisement-control`;
  `crates/control/tests/announce_v2.rs`, `catalog_v2.rs`; `crates/server/tests/announce.rs`,
  `schedule_runtime.rs`; `crates/fetch/tests/http.rs`; `docs/control-plane.md`,
  `docs/inputs.md`, `docs/streams.md`, `deploy/compose.yaml`, `deploy/README.md`,
  `fixtures/README.md`, `audio-path.conf`
- Builds on: ADR 0016 and 0075 (the catalog and its versions), ADR 0025 (the hand-written
  DNS-SD), ADR 0119 (player sources and now-playing), ADR 0120 (the fetcher and its policy),
  ADR 0124 (the engine; no command plays an arbitrary URL), ADR 0129 (held player sessions),
  ADR 0076 and 0079 (the schedule runtime)
- Numbered 0136 and not the 0133 the track's contract named: a record's number is its pull
  request's (`docs/conventions.md` rule 15), 133 was another pull request's, and this one is
  #136.

## Context

Goal 18 builds a Home Assistant integration against the real server. It needs three things
the server did not have: a way to recognise a server it already knows when its address
changes, the same identifier over HTTP, and a way to play a spoken announcement in a room.

Brief section 4.8: "No arbitrary URL fetch except the input paths the decisions name (UPnP
renders, HA's media and TTS URLs from HA's own address, stored alarm stream URLs)". Goal 16
built the first path and goal 17 the third; ADR 0124 recorded that no command on the control
API plays a URL, and ADR 0129 kept that true by making a stored URL an alarm's only. The
second path is this record's. It has to be a command (the integration speaks the control
API), so the rule ADR 0124 states has to be said precisely: no command plays an ARBITRARY
URL.

K31 asks for announcements that duck the music. The ducking mixer is goal 20's. What goal 18
needs is the command, with playback that is correct and cannot leave a house in a wrong
state.

## What was read

All in this repository at `4d87ee9`, 2026-10-04. No GPL or LGPL source, no reciprocally
licensed hardware design and no external web page was opened; nothing here rests on a
document outside the repository, and no Home Assistant source or documentation was read for
this record (the shapes are the coordinator's contract, which track H codes against).

- The rules: `CLAUDE.md`, `docs/conventions.md` (the table and rule 15),
  `docs/decisions/README.md`, `tools/conventions/check-adrs.sh`.
- The contract: `docs/control-plane.md` ("The commands catalog version 2 adds", "The state
  message", "The refusals", "How the messages travel", "The schedule runtime", "Discovery",
  "What is NOT in this catalog", "What survives a restart"), `docs/inputs.md` (whole),
  `docs/streams.md` (to "How it plays").
- Decisions: ADR 0129 to its decision 3; in part and said so: ADR 0075 (its header and
  decision 1, versions), the index lines of 0120 and 0124. What those records say beyond
  those parts was not read for this one.
- Code: `crates/server/src/playersessions.rs` (whole), `conductor.rs` (the pass, the held
  sessions' ends, the stored requests, the schedule's entry points), `schedule_runtime.rs`
  (`on_command_applied`, `rooms_touched`, `release`, `restore`, the snapshot), `control.rs`
  (the state, `runtime`, `commit`, `apply`, the routes), `main.rs` (the control plane's
  start, the advertiser, the identity, the players and the conductor), `config.rs` (the
  flags' parse and checks), `mediaplayer.rs` (the actions, the request, `open`);
  `crates/control/src/catalog.rs` (the commands, the decoder, the refusals), `rooms.rs`
  (stored sources), `zones.rs` (`apply`, `take`, `resolve`, the runtime's hooks);
  `crates/fetch/src/policy.rs` (whole), `http.rs` (`connect`, `get`), `url.rs` (the type);
  `crates/discovery/src/dnssd.rs` (the advertisement), its vector test and
  `firmware/tests/test_discovery.c` (how both walk `fixtures/discovery/`);
  `crates/control/tests/catalog_v2.rs`, `crates/server/tests/alarm_stored_sources.rs` and
  `common/mod.rs`; `crates/ctl/tests/commands.rs` and the grammar's `soloist` noun;
  `crates/server/src/soloist.rs` (its header's lines on a receiver no group plays).

## Decisions

### 1. `GET /api/server`: who the server is

One more read-only route on the control listener, answered by a worker like `GET /api/state`:
`200`, `application/json`, one message in the canonical encoding, members in this order:

```json
{"v":2,"t":"server","id":"chorus-server-0123456789abcdef","software":"chorus-server 0.1.0","catalogs":[1,2],"announce_origins":["http://ha.example:8123"]}
```

It is not a state. Nothing changes it while the server runs, nothing fans it out, the serial
does not know it, and no state vector moved. `fixtures/control/v2/server.json` and
`server-no-origin.json` pin it; `ServerInfo::encode` is in the pure crate so the vector test
needs no server.

`id` is `chorus-server-` and the 16 hexadecimal digits of the fingerprint of the server's
long-term public key (`noise::fingerprint`: the first 8 bytes of the key's SHA-256): 30
characters, inside the route's rule of 1 to 64 lower-case ASCII letters, digits and hyphens.
It is the identity the UPnP renderers' UDNs already derive from (ADR 0125). The prefix says
what kind of thing the id names, because an adopted speaker's id is `chorus-` and hexadecimal
digits too.

**A server with no identity directory.** The server already refuses to start with nowhere to
keep its key unless it is given `--ephemeral-identity`, so every running server has a key and
the route always answers a valid, non-empty `id`. An `--ephemeral-identity` server makes its
key anew at every start, and its `id` is new with it: a throwaway run IS a new server, and
that is what `--ephemeral-identity` says about the audio sessions' pins too. The id is not
made stable by other means (a file of its own, the `--server-id` label), because a second
identity that survives where the key does not would let a controller believe it is talking
to the server it adopted when the endpoints would refuse that server's key.

The identity is now loaded before the advertiser is opened (it was loaded after the audio
thread started), because the control service's TXT record needs the id. No thread exists at
either point; the refusals' order moved only for a configuration that is wrong in two ways at
once.

### 2. `id=` in the control service's TXT record

`_chorus-ctl._tcp.local.` carries `v=<catalog version>` then `id=<id>`, the same value. A
controller that discovers the service reads the id without connecting, so a known server at
a new address is recognised and a second server is told apart. The audio service's record is
unchanged: endpoints pin the server's key in the handshake and need no id, and its fixtures
are shared with the C resolver.

No fixture pinned the control service's advertisement before (only its browse query), so
`fixtures/discovery/advertisement-control` is new: the packet, generated by `make
discovery-vectors`, and what a resolver makes of it. `fixtures/discovery/` is a shared
directory (`check-shared-fixtures.sh`), and both walkers discover vectors from it, so the C
resolver's test resolves the new packet too (run: 100 checks, 0 failed).

### 3. `announce`, and the origin list

```json
{"v":2,"t":"announce","target":"kitchen","url":"http://ha.example:8123/api/tts_proxy/abc.mp3","volume":0.300}
```

`target` is a room, a saved group or a formed group. `url` is held by the decoder to the
shape a stored `url` source has (ADR 0129). `volume` is optional.

**The origin list.** `--announce-origin <scheme://host[:port]>`, repeatable, is the list of
origins a clip may come from. An origin is a scheme (`http` or `https`), a host and a port,
the scheme's default when none is written, and nothing else: a path, a query, userinfo or a
port that is not one is refused at start, by name. The comparison is exact on all three; a
host is compared as text, ASCII case-insensitively, and never resolved to compare it,
because the question the brief asks is whether the URL is from "HA's own address" as the
operator wrote it, and a comparison of resolved addresses would let any name that resolves
to the same machine through. With no origin configured every `announce` is refused, field
`url` (`no-announce-origin`).

So ADR 0124's rule stands, said precisely: no command plays an ARBITRARY URL. The sentences
that said "no command plays a URL" (`docs/control-plane.md`, `docs/inputs.md`,
`docs/streams.md`, the module comment of `playersessions.rs`) say that now.

**The check is made three times, on purpose.** The room model checks the URL's origin with
its own small reader (`Origin::of_url`), which is what the vectors pin and what refuses with
the list in the words. The announcer checks it again with the fetcher's own URL parser,
because that is the parser that decides where a connection goes, and a difference between
two readers of one URL is where this kind of rule is usually broken (the allowed origin
written as userinfo, `http://ha.example:8123@elsewhere.example/`, is refused by both). And
the fetch is HELD to the origins: `chorus_fetch::Policy::origins`, asked in `connect`, where
every connection of a fetch is made.

**Redirects.** The fetcher follows redirects (301, 302, 303, 307, 308, at most 5). Without
the third check an allowed URL that answers with a redirect would make the server fetch
from anywhere the fetch policy allows, and the origin list would be a formality. With it, a
redirect inside the origin is followed and one that leaves it fails the fetch (`refused:
origin <scheme>://<host>:<port> is not one this fetch is held to`) before the name is
resolved; an HLS playlist's segments are held the same way. The fetch policy's address rules
(loopback, link-local, the server's own ports) still apply to what the origin resolves to.
Every other fetch (a cast, an alarm's stream) is held to no origin, as before.

**The refusals**, each naming its field: `target` (unknown; or an alarm is ringing in a room
that would hear it), `url` (the shape; no origin configured; an origin not on the list),
`volume` (out of range), `t` (`no-players`: no `--players`; `no-free-player`: all in use),
`source` (no stream slot free, the slots' own refusal). `t` is the field the catalog already
uses for "this server cannot do that" (`no-receivers`, `no-firmware-dir`). The target is
looked at before the URL, the URL before the players.

### 4. Interrupt and restore now; the mixer in goal 20

The clip plays through a held player session (`PlayerSessions::play_held`, owner
`announce:<n>`, `via` `announce`, title `Announcement`), the way ADR 0129 plays an alarm's
stored URL: the announcer, not a `take` command, gives the group the player, and when the
clip ends the session touches no group and its caller decides what the group plays next.

- **Where it plays.** In the group the target resolves to as things stand: a room's group
  (so everybody in it hears the clip and nobody is regrouped), a formed group, or a saved
  group, which is taken first when it is not active (K78), as any play on a saved group
  does, and stays formed afterwards. Splitting a room out of its group for a few seconds and
  putting the group back together was rejected: a `take` of a room moves its source and its
  now-playing record, and restoring a live group that dissolved needs a new id, so "exactly
  as it was" could not be promised.
- **What is put back.** `Zones::announce_begin` returns the group, the source it had and,
  for each room whose volume was set, the volume before and the volume given (clamped to the
  room's effective limit by the same `set_volume` every path uses). At the end the group
  that plays the player now gets the source back, and each room whose volume is still the
  one the announcement gave it gets its own back: a volume a person changed during the clip
  is that person's.
- **A source that cannot be put back.** A player is given back the moment its group stops
  playing it (a cast's session ends there), and a Spotify receiver no group plays is paused
  by its manager. A group that was playing either goes to `none`, never back to a source
  nothing drives.
- **Who does it.** A control worker starts the announcement inside the command, so "no
  player is free" is the command's answer and not a log line. The conductor ends it
  (`Announcer::settle`, once per pass, after the schedule ran), because it is the thread
  that takes the held sessions' ends. No thread was added. `settle` reads the room model
  rather than trusting an event: an announcement whose group no longer plays its player was
  displaced; one whose session is gone while the group still plays the player ended or
  failed; one older than the bound is cut.
- **The bound.** A clip is cut after 10 minutes (ASSUMED: far above a spoken announcement,
  short enough that a URL that turns out to be an endless stream does not hold a room).
- **Not a person's command.** The schedule runtime is told of every person's command, and
  one naming a room ends an autoplay's hold on it. An announcement is temporary and restores
  what it changed, so the runtime is not told; an autoplay or a sleep timer that ends during
  the clip changes the group as it would have, and the announcement is then displaced.
- **Alarms.** An alarm must still wake. An announcement for a room an alarm is ringing in is
  refused (`target`): interrupting it would take the group from the alarm's own source, and
  an alarm whose stream is a held player would lose it. An alarm that rings DURING an
  announcement takes its rooms as it always does; its snapshot of the rooms then names the
  announcement's player and the announcement's volume, so the conductor tells the runtime
  (`Runtime::announcement_over`) and the snapshot is rewritten to what played, and the
  volumes the rooms had, before the announcement.
- **One epoch counter for held plays.** An alarm's play and an announcement's can follow each
  other on one player, so their epochs come from one counter (`PlayerSessions::held_epoch`),
  above every renderer's as before: a report left over from one held play is older than the
  next, whoever started either.

The mixer (K31, goal 20) replaces this decision's playback: the clip mixed over the group's
audio, ducked, with nothing interrupted. The command, its origin rule and its refusals stay.

### 5. The catalog version stays 2

ADR 0075's rule is that a command is written at the lowest version that declares it and that
a v2-only command at `"v":1` is refused as unknown at that version; a new version is for a
change a v2 peer would misread. `announce` is a new command with new fields: a build without
it rejects it by name (`'announce' is not a command in catalog version 2`), one message, the
session stays open, and no existing message changed meaning. That is how goals 12 to 17
added commands (`sound`, `av_trim`, the speaker and firmware commands, `source_store`,
`playback`) at version 2. `GET /api/server` is a route, not a catalog message a peer sends;
a build without it answers `404`, which a controller reads as "an older server". The state
message did not change: the origin list is configuration and is served on the route.

### 6. chorusctl

No `chorusctl announce` verb. The grammar table, the help text held byte for byte in
`docs/chorusctl.md`, the rows and the vector cases make a verb with a flag a change of five
files, and the command exists for the home automation, which speaks the API. A person tests
it with one `curl`. It is a follow-up, not a refusal.

## Limitations

- No mixing and no ducking: the music stops for the clip.
- Interrupted, not paused: a cast or a Spotify receiver playing in the group does not come
  back (the group plays `none`); the stream, a line-in and a chime do.
- One player per announcement: a server whose players are all in use refuses
  (`no-free-player`), and a group that is casting needs a second player for the clip.
- A room target in a multi-room group is heard by the whole group.
- A second announcement in the same group replaces the first on its player (the first clip
  is cut, not finished).
- A failed fetch is answered `200` and fails afterwards (the fetch runs on the player's
  thread); the reason is in the server's log, not in the state.
- A volume the announcement set is persisted like any volume: a server that stops during a
  clip starts again at the clip's volume, with the group on its configured source.
- The 10 minute bound is tested nowhere on the real binary (it would need a ten minute test
  or a flag that exists only for tests).
- An alarm ringing during an announcement is tested in the runtime's modelled tests
  (`schedule_runtime.rs`), not on the real binary.
- An `--ephemeral-identity` server's `id` changes at every start.
- Nothing here was heard on hardware.

## What changed in committed tests and fixtures

No committed vector moved. New: `fixtures/control/v2/announce`, `announce-volume`, `server`,
`server-no-origin`, `error-announce-origin`, `error-announce-no-origin`,
`error-announce-target`, `error-announce-url`, `error-announce-volume`;
`fixtures/discovery/advertisement-control`. `catalog_v2.rs` learned the `server` message
type and the `announce_origins` line; `crates/fetch/tests/common` names the policy's new
field; `media_player.rs`, `conductor.rs` and `upnp.rs` name a play request's new field.

## The tests' wall clock

`crates/server/tests/announce.rs` (three tests on the real binary, about 6 s side by side)
grades values, orders and log lines, each waited for with a 20 s bound, so it is not in the
wall-clock group. `announce_v2.rs` (six) and the runtime's new test are modelled and run in
milliseconds. Nothing here is timing evidence.

## Follow-ups

- Goal 20: the ducking mixer replaces decision 4's playback.
- `chorusctl announce <target> <url> [--volume v]`.
- A failed announcement shown in the state, if the integration turns out to need it.
- A resume for a cast or a receiver an announcement interrupted, once the mixer makes the
  interruption unnecessary.
