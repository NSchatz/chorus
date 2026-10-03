# UPnP AV media renderers

With `--upnp`, every room, every saved group and every live group of a chorus
server is a **UPnP AV media renderer**: a control point on the same network
(a music app on a phone, a media server's "play to" list, Home Assistant's
media player) finds them, plays a URL on one, and the rooms it stands for
play it. Off by default: without the flag there is no thread, no socket and
nothing on the network.

chorus claims no certification. The words are "UPnP AV media renderer"; it is
not "DLNA Certified", and it sends no `DLNA.ORG_PN` profile name. What a
control point sends in DLNA fields is read as plain text.

The record of why it is built this way is
`docs/decisions/0125-the-upnp-av-media-renderers.md`; the protocol half is
`crates/upnp` (`docs/decisions/0121-a-pure-upnp-av-core-written-from-the-specifications.md`).

## What appears on the network

One device per target, each a root device of type `MediaRenderer:1` with
AVTransport:1, RenderingControl:1 and ConnectionManager:1:

| Target | Appears | Its name (friendlyName) | Model name |
|---|---|---|---|
| a room | always | the room's display name | `chorus room` |
| a saved group | always, formed or not | the saved group's name | `chorus saved group` |
| a live group | while it is formed | its rooms' names, in the order of their ids, joined by ` + ` (`Den + Kitchen`) | `chorus live group` |

A target that appears is announced (`ssdp:alive`); one that goes (a live
group dissolves, a saved group is deleted) says `ssdp:byebye`, its
description answers 404, its event subscriptions end, and what it was playing
stops. Renaming a room is byebye followed by alive with a new CONFIGID: the
device is the same one (the same UDN) under a new name. The server follows
the control state, so it makes no difference whether the app, `chorusctl` or
a speaker's buttons made the change.

**Identities are stable.** A device's UDN is a version 5 UUID over the
server's identity (the fingerprint of its persisted key, the `key=` of the
`identity` line it prints at start) and the target: `room:<id>`,
`group:<saved group id>` or `live:<member room ids, sorted, joined by +>`. So:

- a restart keeps every UDN (which is why `--upnp` refuses
  `--ephemeral-identity`: give `--identity-dir` or `--state-file`);
- two chorus servers on one network have different UDNs for a room of the
  same id;
- a live group that re-forms from the same rooms is the same device again,
  whatever id the control plane gave it this time (`live-1` is reused);
- a rename changes the name, never the UDN. Deleting the server's key file
  changes every UDN, and control points then see a household of new devices.

## Turning it on

```
chorus-server ... --control-listen 0.0.0.0:4020 --state-file /var/lib/chorus/state \
    --slots 4 --players 2 --upnp
```

`--upnp` needs the control plane, at least one stream slot and at least one
player, and refuses to start without each, naming it.

| Flag | Default | What it is |
|---|---|---|
| `--upnp` | off | turn the renderers on |
| `--upnp-listen <addr:port>` | `0.0.0.0:4030` | the one HTTP port of every renderer: descriptions, control (SOAP) and event subscriptions. A literal address; port 0 only on loopback (tests) |
| `--upnp-workers <n>` | 4 | HTTP worker threads, 1 to 16 |
| `--upnp-callback-subnet <cidr>` | this host's own subnets | where event callbacks may point; repeatable. See "What is refused" |
| `--upnp-ssdp-port <port>` | 1900 | the UDP port of the discovery socket. 1900 is the standard; anything else is for tests, or for a host where another program holds 1900 (below) |
| `--upnp-ssdp-group <addr:port>` | `239.255.255.250:1900` | where discovery notifications are sent. Anything else is for tests |
| `--players <n>` | 0 | how many things can play at once (below) |
| `--media-allow-loopback` | off | lets the players fetch from this machine's loopback. For tests and development only, never a deployment: it would let anybody on the network make the server fetch from its own host |

**The firewall.** Control points connect to the HTTP port, TCP 4030 unless
`--upnp-listen` says otherwise, and discovery uses UDP 1900 (multicast
239.255.255.250). `docs/proposals/P6-casting-receivers.md` records that the
homelab host firewall already accepts UDP 1900 from the LAN and that a
host-network service's TCP port has to be listed there: adding TCP 4030 is
the owner's step, through a homelab change in goal 17, not something this
repository does. The server also connects out: to the media URLs it is given,
and to each subscriber's event callback.

**How many can play at once.** A renderer plays through one of the server's
network media players (`--players`, at most 16), and each playing target also
needs a stream slot (`--slots`). The players are a pool: a renderer takes one
when a control point gives it a URL and keeps it while it plays, and for 30
seconds after it stops (so that Stop, then a new URL, then Play finds it
ready); then it goes back. With every player in use, `SetAVTransportURI` and
`Play` fail with UPnP error 501 (Action Failed) and the log says `no free
player`. A house that plays three things at once in different rooms wants
`--players 3` and `--slots` of at least 3.

## What playing does

- **Play takes the room** (K78). Playing on a room's renderer makes that room
  play it, alone (a room that was grouped leaves its group). Playing on a
  saved group's renderer forms the group and plays in all its rooms. Playing
  on a live group's renderer plays in the rooms it has. A renderer whose
  rooms were taken by something else (another renderer, the app, an alarm,
  a line-in starting) goes STOPPED and says so to its subscribers.
- **Gapless.** A control point that sends the next track ahead
  (`SetNextAVTransportURI`) gets it joined to the current one with no gap and
  no overlap: the two files' samples follow each other in the stream
  (`docs/measurements/gapless-join-host.md`). At the boundary the renderer
  stays PLAYING and sends one event in which the current URI and its
  metadata become the next track's and the next URI becomes empty. That it
  stays PLAYING is chorus's reading: the AVTransport specification says what
  the URI variables do at that moment and is silent about the transport
  state (`docs/decisions/0121-...`). A next URI can be replaced or cleared
  until its first samples are on their way to the speakers, at most a second
  before it is heard.
- **Metadata.** The title, artist, album, artwork URL and duration the
  control point sends (DIDL-Lite) become the room's now-playing record, with
  `via` set to `upnp`: the app, `chorusctl`, MQTT (`docs/mqtt.md`) and
  anything else that reads the control state show what was cast. Where the
  control point sends none, the file's own tags are used, and an internet
  radio's stream title replaces the title as it changes. The metadata text
  itself is handed back to control points exactly as it was sent.
- **Volume and mute** go through the control plane, so a room's limit and
  quiet hours hold (I10, K81): a control point that asks for 80 in a room
  limited to 60 gets 60, reads 60 back and is sent an event saying 60. UPnP's
  0 to 100 is the room's volume in hundredths. A group's volume is the
  control plane's group volume (each room scaled, each under its own limit);
  for a saved group that is not formed, every room is set to the value. There
  is no group mute in the catalog, so muting a group mutes each of its rooms,
  and a group reads as muted when all of them are. Changes made elsewhere
  reach UPnP subscribers as events.
- **Stop, Pause, Seek, Next.** Seek is by time (`REL_TIME`) where the media
  server supports range requests; Next skips to the next URI when one was
  given, and is refused otherwise.

## Formats

MP3, FLAC, Ogg Vorbis, Ogg Opus, ALAC in MP4, WAV and raw PCM (`audio/L16`),
over `http` and `https`, as files or as live streams (`docs/decoders.md`,
`docs/streams.md`). **AAC is not played**, in any container: the URL is
accepted, and the renderer then goes STOPPED with `TransportStatus`
`ERROR_OCCURRED`; the server's log says `unsupported: aac`. The format list a
control point is told (`GetProtocolInfo`) has no AAC in it.

## What is refused

- **Media URLs** (`docs/streams.md`): anything but `http` and `https` (error
  716); and, when the player goes to fetch, an address on this machine's
  loopback, a link-local address, the server's own ports, and a redirect to
  any of those. The refusal reaches the control point as STOPPED with
  `ERROR_OCCURRED`.
- **Event callbacks** (the CallStranger rule, CVE-2020-12695, and stricter):
  a subscription is taken only when every callback URL is `http`, names a
  literal address, that address is the subscriber's own (where the SUBSCRIBE
  came from), and it lies in one of `--upnp-callback-subnet`. The default
  list is the IPv4 subnets this host is directly attached to, read from the
  kernel's routing table at start and printed on the `upnp renderers` line;
  give the flag to narrow it (on a host with container bridges the default
  includes those). If the routing table cannot be read the list is empty and
  every callback is refused until a subnet is given. **A callback on loopback
  is allowed only when `--upnp-listen` is itself a loopback address**, which
  is how the tests run; a server listening on the LAN refuses it.
- **Requests**: a body over 64 KiB, a request that takes more than 5 seconds
  to arrive, XML with a DOCTYPE, `M-POST`, a body that is not `text/xml`.
- **Searches** from one address faster than 8 a second are not answered.

## A host that already runs an SSDP program

The discovery socket binds UDP port 1900 on its own. Rust's standard library
cannot set `SO_REUSEADDR` before binding, chorus allows no `unsafe` for it
and adds no dependency for it, so **if another program on the host holds UDP
1900** (a media server's own DLNA service, for example, on a host where both
use the host's network), the server refuses to start with exit code 10 and a
message naming the port. Two ways out, both the owner's to choose:

1. turn the other program's SSDP off, or give chorus an address of its own;
2. `--upnp-ssdp-port <port>` with a port from 49152 to 65535. The renderers
   then announce themselves on the multicast group as usual, with
   `SEARCHPORT.UPNP.ORG` naming that port (UDA 1.1 section 1.2.2), and answer
   searches sent to it. They do **not** hear multicast searches, which go to
   port 1900: a control point finds them from the announcements (at start,
   and again every 10 to 15 minutes) or by searching the announced port, not
   by its own search. Some control points will list them late or not at all.

This was not tried on the homelab host; it is an open point for the
deployment.

Other limits of the discovery socket, for the same reason (`std` only): it
joins the multicast group on one interface, the one `--upnp-listen` names or
else the host's default, and it sends there. A host with several LAN
interfaces is announced on one. IPv4 only.

## Threads and shutdown

`4 + W` threads, made at start with the rest and never after:
`upnp-ssdp`, `upnp-acceptor`, `upnp-events`, `upnp-manager` and
`upnp-worker-0` to `upnp-worker-<W-1>` (`docs/control-plane.md`, "The thread
population"). One thread sends every event, so a subscriber that has gone
away without unsubscribing costs the others one connect timeout (1 second)
when it is first found dead, and its events are then dropped for 30 seconds
at a time while its subscription runs out; subscribers that answer are
served first.

When the server ends by itself it says byebye for every renderer. A killed
process says nothing; control points drop its renderers when their
advertisements expire (30 minutes).

BOOTID is the wall clock's second at the moment a device appears (larger each
time, as UDA 1.1 asks). A server restarted within the same second would
repeat it.

## What was tested, and what was not

Tested (`crates/server/tests/upnp_control_point.rs`, against the real server
binary over real sockets on loopback): discovery, descriptions, every action,
eventing, gapless playing on each of the three kinds of renderer (a room's, a
saved group's and a live group's: the same two tracks, the same assertions on
the events at the boundary, and a client session capturing in every room, one
for the room and one in each of the two rooms of each group, with the PCM
each room received compared sample for sample and, in a group, the two rooms'
captures required to sit in chunks of the same sequence and timestamp),
metadata into the control state of every such room, K78, volume clamping, every format,
the refusals above, identities across a restart, byebye, and sixteen
renderers at once.

**Not tested: any control point application.** The control point in that test
is a script. No phone app, no desktop player and no Home Assistant was run
against these renderers; that is goal 17's work. Also not tested: multicast
on a real interface (the test sends its searches to the server's port and
receives the announcements on a loopback socket), a second host, and the
`SEARCHPORT` route with a real control point.
