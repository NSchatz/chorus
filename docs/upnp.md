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

Since goal 17 every renderer also offers the **OpenHome** services, so a
control point can hand over a whole queue and go to sleep: see "OpenHome"
below (`docs/decisions/0128-openhome-services.md`).

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
| `--upnp-openhome <on\|off>` | `on` | whether every renderer also offers the OpenHome services ("OpenHome" below). `off` leaves the UPnP AV device exactly as goal 16 made it |
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

## OpenHome

UPnP AV has no queue: the control point holds the playlist and pushes one URI
and the next, so the music stops when the phone sleeps. OpenHome is the open
answer to that: the device holds the queue. With `--upnp-openhome on` (the
default when `--upnp` is given) every renderer, a room's, a saved group's and
a live group's, also offers five OpenHome services, and a control point that
speaks OpenHome (`docs/proposals/P6-casting-receivers.md`, Option B) inserts
tracks into the renderer's Playlist, says Play, and can leave. chorus walks
the list itself and joins each track to the next without a gap.

OpenHome has no specification document. Its definition is the service XMLs
and the reference implementation, both MIT: ohPipeline
(`github.com/openhome/ohPipeline` at commit
`cccd06dd49ab154f43e1f24e009fe066a14bf15f`) and ohNet
(`github.com/openhome/ohNet` at `b16816876a88e5f1783225114470025458eb1b61`),
read 2026-10-03. Every rule below cites a file of one of them at those
commits; `crates/upnp/src/openhome/` cites line numbers.

### What is offered

| Service | Announced as | What it does |
|---|---|---|
| Product | `urn:av-openhome-org:service:Product:2` | the device's names, its sources and which is selected, standby |
| Volume | `urn:av-openhome-org:service:Volume:2` | volume, mute, and `VolumeLimit` |
| Info | `urn:av-openhome-org:service:Info:1` | what plays now: URI, metadata, codec details |
| Time | `urn:av-openhome-org:service:Time:1` | the track's duration and the position in seconds |
| Playlist | `urn:av-openhome-org:service:Playlist:1` | the queue the device holds |

The service ids are `urn:av-openhome-org:serviceId:<Name>` (ohNet
`OpenHome/Net/Service.cpp`), the tables are ohPipeline's
`OpenHome/Av/ServiceXml/OpenHome/{Product2,Volume2,Info1,Time1,Playlist1}.xml`,
transcribed in `crates/upnp/src/openhome/tables.rs`.

**Why these versions.** ohPipeline today implements Product 4, Volume 4 and
Playlist 2 (`OpenHome/Av/ProviderProduct.cpp`, `ProviderVolume.cpp`,
`Playlist/ProviderPlaylist.cpp`). chorus announces the lowest version whose
every action it really performs: Product 3 adds `StandbyTransitioning` and
Product 4 image URIs and a changed `Product` response, Volume 3 and 4 add
per-channel offsets, trims and the no-unmute trio, and Playlist 2 adds
`DeleteMultiple` and `Move`; chorus has none of the first group and has not
built the last two. A higher version is a superset, so raising one later is
data plus the added actions. **Which versions BubbleUPnP, Kazoo, Lumin or
Linn's application bind is `ASSUMED`** (older control points Product 1,
current ones 2 or higher): nothing read says, and no application was run.

**A lower version is served.** A search for `...:service:Product:1` is
answered, with `Product:1` in `ST` and `USN` (UDA 1.1 section 1.3.3: the
response carries the version of the search; ohNet does the same,
`OpenHome/Net/Device/Upnp/DviProtocolUpnp.cpp`), and an action sent in the
`Product:1` namespace is performed and answered in it. A search for a higher
version than the announced one is not answered.

### The layout: one device

The five services are listed behind the three AV ones in the same
`MediaRenderer:1` device, with the same UDN: eight services in the
description, eleven discovery messages a set instead of six. This is P6's
"on the same devices".

It is not what ohPipeline's own player does: that announces two root devices
with different UDNs, a `urn:av-openhome-org:device:Source:1` carrying the
OpenHome services and a `MediaRenderer:1` carrying the AV ones
(`OpenHome/Av/Tests/TestMediaPlayer.cpp`). **Whether control point
applications accept the OpenHome services on a `MediaRenderer:1` device is
not established**; they are believed to find products by the Product service
type, not by device type (`ASSUMED`). The fallback is recorded, not built: a
second root device per target of type `urn:av-openhome-org:device:Source:1`,
with a UDN of its own derived from the same target key, carrying only the
five services. The layout is one switch in the description builder
(`DeviceInfo.openhome`, `Advert.openhome` in `crates/upnp`), so the fallback
is a contained change if a bench run with a real application shows the
single device is listed twice, or not as OpenHome.

### The Playlist

- A track is a URI and its metadata (DIDL-Lite), which is kept and handed
  back verbatim. `Insert(AfterId, Uri, Metadata)` returns the new track's id;
  ids count up from 1 and are never used twice in a run. At most 1000 tracks
  (`TracksMax`, ohPipeline's own number), then 801.
- `IdArray` is the ids in list order, each a big-endian 32-bit number,
  concatenated, in base64; a change of it is evented at most once in 300 ms,
  so a burst of inserts is one event. `IdArray()` returns it with a token and
  `IdArrayChanged(Token)` says whether the list changed since. `Read` and
  `ReadList` return tracks.
- `Play` plays the current track (`Id`), then every following one, **with no
  control point connected**: ahead of each boundary the renderer gives its
  player the following track, the same join `SetNextAVTransportURI` gets, so
  the files' samples follow each other with no gap and no overlap. At the end
  of the list it stops with the first track cued; with `SetRepeat(1)` it
  wraps and plays on. `SetShuffle(1)` plays every track once in an order
  drawn when it is switched on (804 with fewer than two tracks).
- `Next`, `Previous`, `SeekId`, `SeekIndex` play another track from its
  start; `SeekSecondAbsolute` and `SeekSecondRelative` move inside the
  current one; `Pause`, `Stop`. `DeleteId` of the track that plays moves on
  to the next; `DeleteAll` empties and stops. The faults are OpenHome's: 800
  id not found, 801 playlist full, 802 index not found, 803 seek failed, 804
  shuffle not possible.
- `TransportState` is `Stopped`, `Buffering`, `Playing` or `Paused`. It stays
  `Playing` across a gapless boundary; `Id` changes there.
- **URIs** are `http` and `https` only (anything else is refused at `Insert`
  with 600) and are fetched only by the server's players under the same fetch
  policy as every renderer URI ("What is refused"); the Playlist adds no
  fetcher. The Product's URL and image fields are empty: chorus fetches and
  serves none.
- The list lives in the server's memory. A restart, and a renderer that
  vanishes (a live group that dissolves), lose it.

### Product: the sources

`SourceXml` lists, in order:

| Source | `Type` | Visible | Selecting it |
|---|---|---|---|
| Playlist | `Playlist` | yes | this renderer's queue |
| UPnP AV | `UpnpAv` | no (as in ohPipeline, `OpenHome/Av/UpnpAv/UpnpAv.cpp`) | what AVTransport plays |
| each line-in or TV input the target's rooms offer | `Analog` (a line-in), `Digital` (optical), `Hdmi` (HDMI ARC) | yes | makes the target's rooms play it: the room model's `take` (K78) |
| Spotify | `NetAux` | yes | listed only while the target's group plays a Spotify receiver (a source spelled `soloist:`); selecting it changes nothing |

An input is listed while the control state lists it, which is while its
signal is present; its `SystemName` is `<endpoint>/<input>` and its `Name`
the input's own name (or its label, once inputs carry labels).
`SetSourceIndex`, `SetSourceIndexByName` and `SetSourceBySystemName` select;
an unknown one is 801. `SourceIndex` follows what the rooms really play, so a
source chosen in the app, by an alarm or by a speaker's button is evented to
OpenHome control points too. `Attributes` is `Info Time Volume`: no Radio, no
Credentials, no Pins, no Transport (K64; "What is not offered").

**Standby** is a flag and nothing more: chorus has no power state.
`SetStandby(1)` stops what this renderer plays and reads back `1`; Play, a
source selection and an AVTransport URI clear it. It exists because Product
requires the action and control points call it.

### Playlist and AVTransport on one renderer

A renderer has one player and two transports, of which one at a time drives
it. `SetAVTransportURI` or AVTransport `Play` makes UPnP AV the source: the
Playlist goes `Stopped` (its list and `Id` are kept), `SourceIndex` is
evented. Playlist `Play`, `SeekId` or `SeekIndex` takes it back: AVTransport
goes STOPPED. The transport that does not have the player answers its
queries and changes nothing that plays. This is ohPipeline's rule
(`OpenHome/Av/Source.cpp`, `UpnpAv/UpnpAv.cpp`,
`Playlist/SourcePlaylist.cpp`).

### Volume, Info, Time

- **Volume** is the same volume RenderingControl sets, on the same 0 to 100
  scale, through the same control plane commands, so each service events
  what the other changed. `VolumeLimit` is the room's limit in the same units
  (a quiet hour's while one is active), rounded down, so a control point
  draws the ceiling (K81); for a group it is the average of its rooms'
  limits, which is the most the group volume (an average of rooms each under
  its own limit) can be. Above the limit ohPipeline's rule holds
  (`OpenHome/Av/VolumeManager.cpp`): the request is clamped to the limit and
  succeeds while the volume is below it, and is refused (811) when the
  volume already is there; above 100 it is always 811. `VolumeLimit` is
  evented when the owner changes the limit. There is no balance and no fade
  (`BalanceMax` and `FadeMax` are 0, the setters answer 801), and
  `VolumeMilliDbPerStep` is 0: chorus's volume law is not linear in decibels.
- **Info** says what the target plays, whichever source: a Playlist track or
  an AVTransport URI with the metadata the control point gave and the
  decoder's sample rate, bit depth and codec name (`BitRate` is the decoded
  stream's for a lossless source and 0 for a lossy one); for a line-in, the
  TV or another holder's player, the source's spelling as `Uri` and the
  room's now-playing record as DIDL-Lite.
- **Time** is the duration and the position in whole seconds. `Seconds` is
  evented once a second while playing, not while paused or stopped, and the
  position is read only while somebody subscribes to Time.

### Events

Plain GENA property sets, one property per variable, no `LastChange` (ohNet
`OpenHome/Net/Device/DviSubscription.cpp`): the initial event carries every
evented variable, each later event only the ones that changed. Booleans are
`1` and `0`. The events leave from the one `upnp-events` thread; no thread
was added for OpenHome.

### What is not offered

**Radio** (and with it no station directory and no library: K64),
Credentials, OAuth, Pins, Transport, Sender and Receiver (Songcast). The
`Attributes` name none of them, and no search for them is answered.
Playlist 2's `DeleteMultiple` and `Move` are not offered either.

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

The OpenHome services (`crates/server/tests/openhome_control_point.rs`, the
same way): the description with eight services and their SCPDs; searches by
version; an action in `Product:1`; each service's initial event; a three-track
Playlist played through with no action from the script after Play, the room's
captured audio being the three files joined sample for sample with both joins
inside a chunk, `TransportState` unbroken, and `Id`, Info and Time following
each track; Next, Previous, the seeks, Pause, DeleteId, Repeat, DeleteAll and
their faults; Time's event once a second; the source list with a line-in as
`Analog` and an optical input as `Digital`, and a source switch that takes
the room off the Playlist; Volume against a room limit of 0.600 and the
limit's own event; the AVTransport takeover both ways; a source change made
in the control plane; standby; and `--upnp-openhome off`. On a room's
renderer; the group renderers run the same code and were not walked again.
The line-in in that test is selected and streams no audio, and the `NetAux`
Spotify source is held by a unit of the source list only: when that test was
written no source spelled `soloist:` existed. It does now (`docs/soloist.md`:
the server's receiver manager gives a group that source when the Spotify app
plays on its device), and the two have not been run together.

**Not tested: any control point application.** No BubbleUPnP, Kazoo, Lumin,
Linn or Home Assistant was run against the OpenHome services either: whether
they list a chorus renderer as an OpenHome device in the single-device
layout, and which service versions they bind, is open until a bench run with
one (the owner's queue). The control point in that test
is a script. No phone app, no desktop player and no Home Assistant was run
against these renderers; that is goal 17's work. Also not tested: multicast
on a real interface (the test sends its searches to the server's port and
receives the announcements on a loopback socket), a second host, and the
`SEARCHPORT` route with a real control point.
