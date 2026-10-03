# 0000: every room, saved group and live group is a UPnP AV media renderer on one HTTP port and a fixed set of threads; targets follow the control fanout; identities are UUID version 5 over the server's persisted key; play takes the room through a pooled player; the gapless boundary stays PLAYING; volume goes through the control plane's clamps

- Status: accepted (goal 16, 2026-10-03)
- Decided by: the goal (program section 20, acceptance line C) inside the coordinator's goal-16
  design envelope ("Renderer (track R)"), track `chorus-g16/upnp-renderer`, part R2; every number
  below not cited is ASSUMED
- Implemented in: `crates/server/src/upnp.rs` (new), `crates/server/src/config.rs` (the
  `--upnp*` flags, `--media-allow-loopback`), `crates/server/src/main.rs` (sockets, threads, the
  fetch policy's ports, exit code 10), `crates/server/src/playersessions.rs` (`start_loaded`,
  `suspend`, `in_session`), `crates/server/tests/upnp_control_point.rs` (new),
  `crates/server/tests/control_thread_population.rs`, `audio-path.conf`, `.config/nextest.toml`,
  `tools/image.sh`, `docs/upnp.md` (new), `docs/control-plane.md`,
  `docs/measurements/gapless-join-host.md`
- Builds on: ADR 0119 (player sources, now-playing), ADR 0120 (the fetcher and its policy), ADR
  0121 (the pure UPnP core), ADR 0122 (the decoders), ADR 0124 (the player engine);
  `docs/proposals/P6-casting-receivers.md` (Option A's renderer, U1)

## Context

After ADRs 0119 to 0124 the server had players that could play a URL gaplessly into a room, a
now-playing record in the control state, and a pure crate that knows everything about a UPnP AV
media renderer except how to open a socket. Nothing was on the network. This record is the
sockets, the threads and the glue, and the test that holds the goal's line C: "A UPnP AV
renderer per room, saved group and live group plays gapless with metadata and stable identities
to a scripted control point".

The constraints: every thread exists before the scheduling report and none is made later
(`docs/control-plane.md`, "The thread population"); `std::net` only, no `unsafe`, no new
dependency (the envelope); no network I/O inside an action; limits clamp every volume path (I10,
K81); a target is a room, a saved group or a live group (K57, K59) and playing takes the room
(K78); metadata reaches the control state (K65); callbacks obey the CallStranger rule and P6's
stricter one.

## What was read

All on 2026-10-03.

- **No specification text was read by this part.** Every protocol rule applied here is one the
  core already implements and ADR 0121 cites with its section (UDA 1.1 of 15 October 2008, UDA
  2.0 of 17 April 2020, MediaRenderer:1, AVTransport:1, RenderingControl:1 and
  ConnectionManager:1 Service Templates 1.01, AVTransport:3 of 31 March 2013 section 5.4.3.3,
  RFC 9562). They were read here through the core's doc comments (`crates/upnp/src/lib.rs`,
  `avtransport.rs`, `ssdp.rs`, `gena.rs`, `rendering.rs`, `description.rs`, `soap.rs`,
  `lastchange.rs`, `connmgr.rs`, `uuid.rs`, `didl.rs`, `client.rs`) and through the goal's
  research digest `upnp.md`: its sections 1 (SSDP), 4 (GENA), 5 (playback semantics), "Design
  recommendations" and "Test plan for the scripted control point", each with the section
  citations it carries. Where this record quotes a specification it quotes it from there.
- In the repository: ADR 0121's "What was read" and "Decision"; ADR 0124 and the engine's own
  notes (the effect-to-action and report-to-AVTransport tables, the noted race);
  `crates/server/src/mediaplayer.rs`, `playersessions.rs`, `control.rs` (`ControlState`),
  `mqtt.rs` (the model for a thread that follows the fanout and for a clean stop), `main.rs`,
  `config.rs` (`MqttFlags`), `crates/discovery/src/net.rs` (the multicast socket model),
  `crates/server/tests/media_player.rs`, `control_thread_population.rs` and `common/mod.rs`;
  `docs/control-plane.md` (groups, `take`, group volume, the thread population);
  `docs/proposals/P6-casting-receivers.md` (the callback rule, the firewall note);
  the goal's code survey, sections 1, 4 and 8; the goal-16 design envelope.
- `/proc/net/route` and `/proc/sys/net/ipv4/ip_local_port_range` on the development host (the
  format of the first, the value `32768 60999` of the second).
- **Not read, by rule:** no source file of any GPL or LGPL project was opened: not
  gmrender-resurrect, upmpdcli, MPD, GUPnP, Rygel, Platinum, Kodi, VLC, Snapcast or squeezelite.
  No other renderer's or control point's source was opened by this part at all.

## Decision

### One renderer per target, one port, fixed threads

Every target is a root device of its own (UDN, six discovery messages, description, three
services). All of them are served on one HTTP listener, `--upnp-listen`, distinguished by the
UUID in the path (`/upnp/<uuid>/...`, the core's `description::route`). An unknown or vanished
UUID is 404.

`--upnp` creates `4 + W` ordinary threads with the rest of the population, before the
scheduling report, and none later:

- `upnp-ssdp`: the discovery socket. Alive on appearance and on the core's `Announcer`
  schedule, byebye on vanishing and on the run's end, M-SEARCH answers through
  `search_responses`, spread over the MX window by `response_delays` and limited per source by
  `SearchLimiter`.
- `upnp-acceptor` and `upnp-worker-<i>` (`--upnp-workers`, default 4, at most 16): one request
  per connection; the head at most 16 KiB, the body at most 64 KiB by `Content-Length` or
  chunked, the whole request within 5 s; HTTP/1.0 and 1.1; every response has a
  `Content-Length` and closes. A connection that arrives with every worker busy is answered 503.
- `upnp-events`: every outbound NOTIFY, with a 1 s connect timeout and 2 s write and read
  timeouts.
- `upnp-manager`: follows the control fanout and the players' reports.

The manager is its own thread and not folded into `upnp-events`, because the event thread can be
inside a connect to a subscriber that does not answer, and a track boundary or a vanished
target must not wait behind that.

One mutex holds the table of renderers. Nobody does network I/O while holding it: a worker reads
the whole request first and writes the response after; the event thread takes what is due and
delivers with the lock released. An action changes the AVTransport state machine, sends the
player a command over its channel, perhaps applies a control command (in memory, plus the state
file's write), and returns.

Exit code 10 is new: the renderers' sockets could not be opened (before any thread exists).

### Targets follow the control fanout

The manager subscribes to the control fanout as the MQTT publisher does, takes the latest state,
and brings the table to it: `zones[]` are the rooms, `saved_groups[]` the saved groups (always,
formed or not), and `groups[]` entries of kind `live` the live groups. A new target is a new
renderer; a vanished one is removed (its description then answers 404, its subscriptions are
gone, what it was playing stops); a target whose description changed (a rename) keeps its UDN
and its transport state, loses its subscriptions and gets a new CONFIGID and a larger BOOTID.
The discovery thread compares the table's (UDN, BOOTID) pairs with what it has announced: a pair
that is gone gets the byebye set twice, a new pair a new announce schedule. A state older than
the one the table was made from (by its `serial`) is ignored, so the table never steps back.

friendlyName: a room's display name; a saved group's name; for a live group its rooms' display
names in the order of their ids joined by " + ". modelName says which kind.

### Identity

`UDN = uuid::udn(CHORUS_NAMESPACE, server_id, target)` (ADR 0121), with `server_id` the
fingerprint of the server's persisted public key (the `key=` of its `identity` line) and
`target` `room:<id>`, `group:<saved id>` or `live:<sorted member ids joined by +>`.

The key is what a restart keeps and what no other server has; `--server-id` is a label whose
default is the same on every server, so it is not used. A run with `--ephemeral-identity` has a
new key every time and would show control points a household of new devices at every start, so
`--upnp` refuses it by name. Deleting the key file changes every UDN.

A live group's control-plane id (`live-<n>`) is reused by whichever live group forms next, so
it is not part of the identity: the member set is. The same members re-formed are the same
device, with a larger BOOTID.

BOOTID is `ssdp::boot_id(wall clock seconds, the last value this UDN used in this process)`: it
rises on every appearance within a run, and across restarts as long as a restart takes a second.
CONFIGID is the core's hash of the description and the SCPDs, the same after a restart.

### The player pool and "take the room"

A renderer takes a player from `Players` when an action needs one (SetAVTransportURI with a URI,
or Play) and none is held. A trial of the action on a copy of the AVTransport state says whether
it would be accepted and whether it needs a player, before anything changes, so a refused
action never holds a player and a missing player never half-applies an action. With no player
free the action fails with **501 Action Failed** (UDA 1.1 table 3-3: "current state of service
prevents invoking that action"). AVTransport's own 705 "Transport is locked" is a device's hold
switch and 715 "Content BUSY" is about the content, so neither says what happened.

Play (the core's `Start` effect) issues `take <target> player:p<i>` through
`ControlState::apply`, by `PlayerSessions::start_loaded`: a room target takes the room out of
any group, a saved group target forms the group, a live group target is its current id. A
renderer that is already playing in its group and gets a new URI does not take again, so rooms
that joined meanwhile stay.

When another party changes what the rooms play, `PlayerSessions::reconcile` finds the player
without a group and gives it back; the manager sees the renderer's player gone and stops its
AVTransport, which events STOPPED.

A player is kept through Stop, the end of the media and a failure, loaded, for 30 s
(`STOPPED_HOLD_MS`), and then unloaded and given back; it is given back at once on an eject (an
empty URI), when the target vanishes and when the rooms play something else. The reason for the
hold: control points send Stop, SetAVTransportURI and Play one after another, and a player that
was given back must be unloaded before it can be taken again, which is not instant.
`PlayerSessions` gained `start_loaded` (start without loading again, so a queued next URI
survives), `suspend` (end the session, keep the player) and `in_session` for this; its `play`
and its release-at-the-end stay for the other in-process callers ADR 0124 names.

The epoch a renderer sends with each action is a number of its own for each player it takes, in
the upper half, and the AVTransport epoch in the lower: a report of a player's previous holder
can never be taken for the present one's.

### Gapless, and PLAYING through the boundary

SetNextAVTransportURI becomes the engine's `QueueNext`. The engine's `Boundary` report, sent
when the next track's first frame has been taken by the audio thread, becomes the core's
`track_boundary`, under one hold of the table's lock: the moderation queue then yields one
LastChange in which AVTransportURI, CurrentTrackURI and both metadata variables take the next
track's values, the durations change and NextAVTransportURI becomes empty.

TransportState stays PLAYING, with no STOPPED and no TRANSITIONING evented between the tracks.
That is the core's inference, recorded in ADR 0121: AVTransport:1 section 2.4.2.3 (and
AVTransport:3 section 5.4.3.3) says what the URI variables do when "the playback of the current
resource finishes" and says nothing about the transport state or the events. The test asserts
what chorus does, on the events a subscriber received.

The engine's noted race: a SetNextAVTransportURI (or a clear) that arrives after the join was
written and before it is audible is, to the engine, the track after the joined one. `Boundary`
names the URI that became audible; when it is not AVTransport's next URI, the manager puts the
joined URI (with the metadata it was queued with) back as next, performs the boundary, and
queues the newer one again behind it, in one change. How late is too late to replace a next
URI: once its first frames are in the player's ring, at most 1 s before they are heard.

### Volume and mute

RenderingControl's SetVolume and SetMute become control commands through `ControlState::apply`:
`volume` and `mute` for a room; `group_volume` for a group that is formed; for a saved group
that is not formed, `volume` on each of its rooms (there is no group to scale). The catalog has
no group mute, so a group target mutes each of its rooms, and reads as muted when all are. The
state the command returns is applied to the table before the action answers, so GetVolume and
the event carry the real, clamped value and the requested one is never evented (the core's
`RenderingControl::report`). Changes made anywhere else reach the table through the fanout and
are evented the same way.

### Metadata

At Play the DIDL-Lite of the current URI (`didl::parse`) gives title, artist, album, artwork
URL and duration as the session's hints, with `via` `upnp`; `PlayerSessions` falls back to the
file's tags and to a stream's title. At a gapless boundary the hints become the next track's.
The metadata text is kept by the core verbatim for the Get actions and the events.

### Security rules

- **Callbacks**: every CALLBACK URL of a SUBSCRIBE must pass `gena::callback_allowed` with the
  requester's address and the configured subnets, else 412. The default subnets are the IPv4
  routes of `/proc/net/route` that have no gateway, without the loopback interface and
  169.254/16: `std` cannot list interfaces, and that file is what Linux offers without `unsafe`.
  Unreadable means an empty list (every non-loopback callback refused until a subnet is given).
  A loopback callback is allowed only when the HTTP listener is itself on loopback
  (`loopback_callbacks_allowed`), which is how tests run; both sides are a unit test.
- **Media**: the engine's fetch policy, to which main.rs adds the renderers' bound HTTP port
  and now the control port as bound (a configured port 0 named nothing before).
  `--media-allow-loopback` exists for the binary-level test, whose media server is on loopback;
  it is documented as never for a deployment and the server prints a note when it is set.
- **Requests**: bounded head, body and time; a DOCTYPE is refused by the core's reader; M-POST
  is 405.
- **Discovery**: at most 8 searches a second are answered per source address.

### The discovery socket, with `std` only

The socket binds `0.0.0.0:<--upnp-ssdp-port>` (the loopback address when the HTTP listener is on
loopback), joins 239.255.255.250 on the interface `--upnp-listen` names (else the default one)
and sends with TTL 2. Three things follow from `std::net`:

- **No `SO_REUSEADDR`.** If another program holds UDP 1900, the bind fails and the server exits
  10 naming the port. `--upnp-ssdp-port <49152..65535>` is the way round that UDA 1.1 section
  1.2.2 allows ("Only if port 1900 is unavailable MAY a device select a different port"): the
  alive messages then carry `SEARCHPORT.UPNP.ORG` and searches sent to that port are answered,
  but multicast searches (which go to 1900) are not heard. Open point for the deployment: the
  homelab host runs another SSDP program (P6); which of the two ways is taken there is the
  owner's, and neither was tried on that host.
- **Which form of search a datagram is** cannot be read from the socket, so it is read from the
  datagram: a `HOST` naming a multicast address (or none) is held to the multicast rule (MX
  required); one naming a unicast address is a unicast search (answered within a second).
- **One interface.** LOCATION carries the listener's address, or, for a listener on every
  address, the local address the route to the peer leaves from (a connected UDP socket's, which
  sends nothing).

`--upnp-ssdp-group <addr:port>` redirects the notifications; it exists so the test can receive
them on a loopback socket.

### The HTTP port

4030 by default: the next in chorus's family (audio 4010, control 4020) and below the
development host's ephemeral range (32768 to 60999), so no outbound connection is handed it
first. The host firewall rule follows it (P6).

### Eventing on one thread

Events are queued per subscriber. Each round delivers to the subscribers in good standing
first. A delivery that fails (no connection within 1 s, no answer within 2 s, a status that is
not 2xx) marks the subscriber for 30 s, during which its messages are abandoned (UDA 1.1
section 4.3.2: "SHOULD abandon sending this message ... but MUST keep the subscription"); its
subscription lives until it expires. A 412 answer drops the subscription. The initial event is
queued by the worker after it has written and flushed the SUBSCRIBE response.

## The test

`crates/server/tests/upnp_control_point.rs`, 17 tests in the `wall-clock` nextest group, against
the real server binary over loopback sockets, written with `chorus_upnp::client`. Its header
maps the research digest's 17 steps to tests and says what is not covered. The room's PCM is
captured by a real client session and compared sample for sample for the gapless pair.

What was not tested: any third-party control point (none was run); multicast (searches are sent
to the server's port and notifications received on a loopback socket, so the group join and the
multicast send are not exercised); a second host; a subscription expiring; the late
SetNextAVTransportURI race; the SEARCHPORT route with a real control point.

## Options not chosen

- **A port per renderer.** More firewall rules, and a live group would open a port when it
  forms. Nothing in UDA 1.1 ties a root device to a port.
- **`SO_REUSEADDR` through `libc` or `socket2`.** `unsafe` or a new dependency, both ruled out
  by the envelope. The cost is the caveat above.
- **A thread per subscriber, or non-blocking connects.** The first breaks the thread contract;
  `std` has no non-blocking connect. The cost: a dead subscriber delays the others by at most
  one connect timeout per 30 s.
- **`PlayerSessions::play` as it was** (take, load and start in one; release at the end).
  AVTransport loads at SetAVTransportURI and starts at Play, and a Stop is usually followed by
  a Play; see "The player pool".
- **Letting a vanished target's audio play on.** Nothing could control it; it stops.
- **`--server-id` as the identity.** Its default is the same on every server.
- **Refusing AAC inside SetAVTransportURI by its protocolInfo.** The media type a control point
  claims is a hint; the decoder decides, and the refusal arrives as ERROR_OCCURRED.
- **Assembling the server in process for the test.** The binary is what ships, and the flags,
  the threads and the restart are part of what line C asks.

## Consequences and open points

- The homelab firewall needs TCP 4030 (the owner's step, goal 17, P6).
- The SSDP port on a host with another SSDP program (above).
- One events thread (above). A second lane for suspect subscribers would remove the one-timeout
  delay at the cost of a thread.
- BOOTID repeats if the server restarts within one wall-clock second.
- A subscription is always granted 1800 s (`TimeoutPolicy::STANDARD`).
- The audio port is in the fetch policy only when it is configured (the policy is built before
  the audio socket is bound); the control and renderer ports are there as bound.
- A renderer that is STOPPED holds a player for up to 30 s; with `--players 1` a second
  renderer gets 501 in that time.
- IPv4 only; one interface.
