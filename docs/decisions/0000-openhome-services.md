# 0000: every UPnP renderer also offers OpenHome Product:2, Volume:2, Info:1, Time:1 and Playlist:1 on the same device, behind one switch; the Playlist is held by chorus and drives the player through a private AVTransport deck, so a queue plays through gapless with no control point connected

- Status: accepted (goal 17, 2026-10-03)
- Decided by: the goal (program section 21; P6 Option B, approved) inside the coordinator's
  goal-17 design envelope (sections 0, 1 and 4), track `chorus-g17/openhome`; every number
  below not cited is ASSUMED
- Implemented in: `crates/upnp/src/openhome/` (new: `tables.rs`, `product.rs`, `volume.rs`,
  `info.rs`, `time.rs`, `playlist.rs`, `mod.rs` with the event tracker), `crates/upnp/src/base64.rs`
  (new), `crates/upnp/src/lib.rs` (`Service`), `description.rs`, `ssdp.rs`, `soap.rs`,
  `crates/upnp/tests/openhome_playlist.rs` (new), `crates/upnp/tests/fixtures.rs`,
  `fixtures/upnp/` (the `openhome` vectors), `crates/server/src/upnp.rs`,
  `crates/server/src/config.rs` and `main.rs` (`--upnp-openhome`),
  `crates/server/src/linein.rs` (one read accessor, `LineIns::kind_of`),
  `crates/server/tests/openhome_control_point.rs` (new), `crates/server/tests/common/upnp_cp.rs`
  (the control point's tools, moved out of `upnp_control_point.rs`), `docs/upnp.md`
- Builds on: ADR 0121 (the pure UPnP core), ADR 0124 (the player engine and its next-URI
  join), ADR 0125 (the renderers); `docs/proposals/P6-casting-receivers.md` (Option B)

## Context

Goal 16 made every room, saved group and live group a UPnP AV media renderer. UPnP AV has no
queue: the control point pushes one URI and the next, so playback ends when the phone sleeps.
P6's Option B is the open remedy: OpenHome's services, where the device holds the queue. The
brief asks for it on the same devices, with no radio directory and no library (K64), with the
room limit visible to the control point (K81), with every URL fetched only through a named
input path (section 4.8), and with no thread made after start.

OpenHome has no specification. What defines it is the service XMLs and the reference
implementation, ohPipeline and ohNet, both MIT, which may be read and cited
(`docs/clean-room.md`).

## What was read

All on 2026-10-03.

- The goal's research digest of OpenHome (the coordinator's research agent read the sources
  below and pinned them; this track worked from its digest and its local copies):
  ohPipeline at `cccd06dd49ab154f43e1f24e009fe066a14bf15f` (MIT: `License.txt`,
  `MitLicense.txt`), files `OpenHome/Av/ServiceXml/OpenHome/Product2.xml`, `Volume2.xml`,
  `Info1.xml`, `Time1.xml`, `Playlist1.xml` (and the higher versions, for what they add),
  `OpenHome/Av/ProviderProduct.cpp`, `Product.cpp`, `Source.cpp`, `ProviderVolume.cpp`,
  `VolumeManager.cpp`, `ProviderInfo.cpp`, `ProviderTime.cpp`,
  `Playlist/ProviderPlaylist.cpp`, `Playlist/SourcePlaylist.cpp`, `Playlist/TrackDatabase.cpp`,
  `Playlist/UriProviderPlaylist.cpp`, `UpnpAv/UpnpAv.cpp`, `MediaPlayer.cpp`,
  `Utils/FaultCode.cpp`, `Tests/TestMediaPlayer.cpp`; ohNet at
  `b16816876a88e5f1783225114470025458eb1b61` (MIT: `License.txt`), files
  `OpenHome/Net/Service.cpp`, `OpenHome/Net/Device/DviSubscription.cpp`,
  `OpenHome/Net/Device/Upnp/DviProtocolUpnp.cpp`, `DviServerUpnp.cpp`. The digest gives the
  line ranges; `crates/upnp/src/openhome/*.rs` repeats them beside each rule.
- Opened by this track itself, in the local copies: `OpenHome/Av/Playlist/SourcePlaylist.cpp`
  lines 268 to 325 (what `Play` does while playing and on an empty list) and
  `OpenHome/Net/Device/Upnp/DviProtocolUpnp.cpp` lines 655 to 705 (the search by service type
  answers a lower version). No code was copied: the tables are transcribed data.
- RFC 4648 sections 3.5, 4 and 10 (base64 and its test vectors), ASSUMED from memory for the
  text and held by the seven vectors of section 10 in `crates/upnp/src/base64.rs`.
- In the repository: `CLAUDE.md`, `docs/conventions.md`, `docs/upnp.md`, ADRs 0121 and 0125,
  `crates/upnp/src/*.rs`, `crates/server/src/upnp.rs`, `playersessions.rs`, `mediaplayer.rs`
  (the actions and reports), `linein.rs`, `crates/control/src/zones.rs` (the state's members,
  the group volume, `offer_input`), `crates/server/tests/upnp_control_point.rs` and
  `alarms_sleep_autoplay.rs` (the scripted line-in), the goal's code survey sections E and H.
- **Not read, by rule:** no source file of any GPL or LGPL project (upmpdcli, MPD,
  gmrender-resurrect, GUPnP, Rygel, Kodi among them). No control point application was run or
  decompiled. The archived OpenHome wiki could not be fetched; where the digest leans on P6's
  reading of it (the source types `Analog`, `Digital`, `Hdmi`), that is said in the code.

## Decision

1. **Five services, these versions.** Product:2, Volume:2, Info:1, Time:1, Playlist:1: the
   lowest version of each whose every action chorus performs. ohPipeline implements Product 4,
   Volume 4 and Playlist 2; what they add is in `docs/upnp.md`. Which versions control point
   applications bind is ASSUMED; a higher version is a superset, so raising one is the table
   plus the added actions.
2. **One device, one switch.** The services are listed behind the three AV ones on the same
   `MediaRenderer:1` device (P6: "on the same devices"). ohPipeline's own player uses two root
   devices. Whether applications accept the single device is not established, so the layout is
   one boolean in the description builder (`DeviceInfo.openhome`, `Advert.openhome`,
   `Service::offered`) and the server's `--upnp-openhome <on|off>`, on by default with `--upnp`.
   **The recorded fallback, not built:** a second root device per target of type
   `urn:av-openhome-org:device:Source:1` with its own UDN carrying only the five services. Off
   leaves the description, the discovery set and the CONFIGID of goal 16 byte for byte (the
   committed AV vectors did not move).
3. **A lower version is served.** `Service::matching` accepts the announced version or a lower
   one of the same service; a search is answered with the searched version in `ST` and `USN`,
   and an action in the namespace it named (`ssdp::matches`, `soap::validate`,
   `soap::build_response_at`). The AV services stay exact: only version 1 exists.
4. **The Playlist owns a private AVTransport deck.** `openhome::playlist::Playlist` is the list,
   the cursor, Repeat and Shuffle, and an `AvTransport` no control point sees. It sets the
   deck's URI and next URI and plays, pauses and seeks it, so it returns the same effects and
   takes the same player reports and epochs as the AVTransport service. The gapless handover
   and its races (a list change between the join being written and being heard) are the tested
   machine of ADR 0121 and 0125, not a second one. The Playlist's own rule at a boundary: the
   player names the URI that became audible, the track is found among those lately handed
   over, and the following track is queued at once, with no control point in the loop.
5. **Two decks, one player.** A renderer's player is driven by AVTransport's machine or by the
   Playlist's. `SetAVTransportURI` and AVTransport `Play` take it for the first; Playlist
   `Play`, `SeekId` and `SeekIndex` for the second; the one that loses it goes STOPPED and
   keeps its state; reports still in flight are dropped by a new epoch base. This is
   ohPipeline's source activation. The transport that does not have the player answers and
   changes nothing that plays.
6. **Sources.** Playlist, UPnP AV (not visible, as in ohPipeline), each input the control
   state lists for an endpoint of the target's rooms (`Analog` for a line-in, `Digital` for
   optical, `Hdmi` for HDMI ARC), and a `NetAux` "Spotify" while the target's group plays a
   source spelled `soloist:` (the spelling of the Soloist track of this goal; tolerated when
   absent). Selecting an input issues the room model's `take` (K78). `SourceIndex` is derived
   from what the rooms play, so a change made anywhere is evented. The state lists an input
   while its signal is present, so that is when it is a source. The state does not say an
   input's kind; `LineIns::kind_of` does, and an unknown kind is `Analog`.
7. **Standby is a flag.** chorus has no power state. `SetStandby(1)` stops what the renderer
   plays and reads back 1; playing, selecting a source and an AVTransport URI clear it. It is
   there because Product requires it and control points call it; it never powers anything
   down and is not persisted.
8. **Volume and its limit.** The same volume as RenderingControl's, through the same control
   commands. `VolumeLimit` is the room's effective limit (a quiet hour's while active) on the
   0 to 100 scale, rounded down. For a group it is the average of its rooms' effective limits:
   the control plane's group volume is the average of its rooms' volumes, each clamped to its
   own limit, so that average is the most it can reach (the design envelope said "the lowest
   of its rooms' limits unless the existing mapping says otherwise"; it does). Above the limit
   ohPipeline's rule: clamp and succeed below it, 811 at it; above 100 always 811.
   `VolumeMilliDbPerStep` is 0 (chorus's law is not linear in decibels); no balance, no fade.
9. **Events.** A `Tracker` per service keeps what was last evented; the event thread sends the
   changed variables of each service in one property set to every subscriber that has had its
   initial event. ohNet keeps the comparison per subscriber; one per service says the same to
   each. `IdArray` is held 300 ms from its first change (ohPipeline's number). Time's position
   is read from the player only while a Time subscription exists, and changes once a second.
   No thread is added.
10. **Chorus's own, where the reference gives nothing usable.** A Playlist URI that is not
    `http` or `https` is refused at `Insert` (600): the fetch policy takes nothing else, and
    the Playlist adds no fetcher (brief section 4.8). Shuffle is a permutation by a seed drawn
    when it is switched on. A queued track that cannot play stops the list on it. Info's
    `BitRate` is the decoded stream's for a lossless source and 0 otherwise. For a source that
    is not a renderer's own track, Info's `Uri` is the source's spelling and its `Metadata` a
    DIDL-Lite item made of the now-playing record.

## What was not chosen

- **Radio, Credentials, OAuth, Pins, Transport, Songcast**: K64 and section 4.8 (Pins and
  Radio fetch from services by themselves). `Attributes` is `Info Time Volume`.
- **Playlist 2** (`DeleteMultiple`, `Move`): not built; announcing 1 is honest.
- **A second state machine for the Playlist's transport**: the deck reuses the tested one.
- **A persisted Playlist**: it lives in memory; a restart or a vanished target loses it.
- **Input kinds in the control state**: the inputs track of this goal owns the catalog; one
  read accessor on the line-ins was the smaller hook.
- **A base64 crate**: thirty lines, held to the RFC's vectors (working agreement 3).

## Limits

- **No control point application was run.** The control point is a script
  (`crates/server/tests/openhome_control_point.rs`). Whether BubbleUPnP, Kazoo, Lumin or
  Linn's application lists a chorus renderer as an OpenHome device in the single-device
  layout, and which versions they bind, is open until a bench run (the owner's queue).
- The binary-level test walks a room's renderer; the group renderers share the code.
- The `NetAux` source is unit-tested only: no `soloist:` source exists on this branch.
- The line-in in the binary-level test is selected and streams no audio.
