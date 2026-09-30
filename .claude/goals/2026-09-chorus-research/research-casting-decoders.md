# Research: casting receivers, Spotify Soloist, decoders, concurrent streams (chorus)

Date: 2026-09-29. Decisions covered: K57, K58, K59, K63, K66, K76, K78, K79, K80, read under K60 (strict
legal bar), K26 (MIT OR Apache-2.0), K33 (no GPL source opened), K56 (no MA, no Sendspin), K64 (inputs
only), K75 (8 rooms). WebSearch was exhausted for this session, so every source is a direct fetch of a
known URL; anything not fetched is marked ASSUMED. Sources are [R#] (list at the end, all read
2026-09-29). **Not legal advice**: legal statements are quoted facts or are labelled "inference".

Summary of recommendations (details and confidence in section 6):

- **UPnP AV renderer**: write it in Rust inside chorus-server (SSDP + device description + SOAP +
  GENA eventing; AVTransport with SetNextAVTransportURI, RenderingControl, ConnectionManager). One
  root device per cast target with a UUIDv5 UDN derived from the target's stable identity. Call it a
  "UPnP AV media renderer"; never "DLNA Certified". OpenHome services as a later, optional layer.
- **Other protocols**: add none now. Every other push protocol is either partner-only (Roon Ready,
  Tidal/Qobuz Connect, Cast, AirPlay MFi) or reverse-engineered (open AirPlay/Cast receivers), or is not
  a cast protocol at all (Snapcast, MPD). One optional legal extra: an OpenHome **Songcast receiver**
  (Linn's open protocol, BSD-2 reference) for "PC audio to a room", only if the owner wants it.
- **Soloist**: one sidecar container `chorus-soloist` (host network) running headless PipeWire plus one
  Soloist process per target, each writing into its own PipeWire pipe-tunnel sink that chorus-server
  reads as a FIFO input; control and metadata over each instance's loopback WebSocket. Alarms can
  start a Spotify URI (`play` with `uri` is documented). Instance limits and household use are
  **not stated** in any public page: an owner step is to read the dashboard T&C clause.
- **Decoders**: server decodes everything (endpoints only ever see PCM/FLAC/Opus, K62). Symphonia
  (MPL-2.0) for MP3/FLAC/Vorbis/ALAC/WAV + Ogg/MP4 containers, libopus (BSD-3) through a MIT/Apache
  binding for Opus decode and encode; C endpoint vendors dr_flac (Unlicense/MIT-0) and libopus.
  **AAC is the one owner call**: US AAC patents are live until about 2028 (baseline) / 2031
  (extensions); recommended default is AAC **off** behind a build feature the owner may enable.
- **Concurrent streams**: measured on this host class (Xeon E5-2680 v4), decoding costs 0.16% to 0.52%
  of one core per stereo stream and Opus encoding about 1.9%; the CPU is not the limit. Proposed
  limit: 8 independent streams (one per room, K75) and at most 16 Soloist instances, with a 2 GB memory
  budget for the Soloist container pending a real Soloist measurement.

---

## 1. K57: UPnP AV media renderer

### 1.1 What a MediaRenderer must implement

| Layer | Requirement | Source |
|---|---|---|
| Device | MediaRenderer:1 requires **RenderingControl:1** and **ConnectionManager:1**; AVTransport:1 is "O" at device level but must exist when the transfer protocol needs it ("If ... [it supports such] transfer protocols, then it must implement the AVTransport service"); HTTP GET pull is such a protocol, so in practice AVTransport is required. Control points use AVTransport InstanceID 0 when PrepareForConnection is not implemented. | [R12] |
| AVTransport:1 | SetAVTransportURI R, **SetNextAVTransportURI O**, GetMediaInfo R, GetTransportInfo R, GetPositionInfo R, GetDeviceCapabilities R, GetTransportSettings R, Stop R, Play R, Pause O, Record O, Seek R, Next R, Previous R, SetPlayMode O, GetCurrentTransportActions O. Metadata arrives as a **DIDL-Lite** fragment (CurrentURIMetaData / NextURIMetaData). State changes are evented through the LastChange variable. | [R11] |
| Gapless | SetNextAVTransportURI "allows a device to 'prefetch' the data to be played next, in order to provide a seamless transition between resources"; when the current resource ends, AVTransportURI takes the NextAVTransportURI value; an illegal next URI leaves the transport STOPPED after the current one ends. | [R11] |
| RenderingControl:1 | ListPresets R, SelectPreset R; GetMute/SetMute, GetVolume/SetVolume, GetVolumeDB/SetVolumeDB/GetVolumeDBRange, GetLoudness/SetLoudness all O (control points expect volume and mute in practice: ASSUMED). | [R13] |
| ConnectionManager:1 | GetProtocolInfo R (the renderer's Sink list is how it advertises formats such as `http-get:*:audio/flac:*`), GetCurrentConnectionIDs R, GetCurrentConnectionInfo R, PrepareForConnection O, ConnectionComplete O. | [R14] |
| Discovery | SSDP (UDP 1900 multicast): NOTIFY ssdp:alive / ssdp:byebye per root device, embedded device and service, M-SEARCH responses, CACHE-CONTROL max-age; device description XML at the LOCATION URL; SOAP 1.1 control; GENA SUBSCRIBE/NOTIFY eventing. From the UPnP Device Architecture (in the OCF zip [R15]); section-level details **ASSUMED** from general knowledge, to be checked against UDA 1.1 when implemented. | [R15] ASSUMED |

Spec availability and rights: OCF hosts every UPnP spec as a free zip; the page says rights are
"SUBJECT TO THE UPNP BYLAWS AND FORUM MEMBERSHIP AGREEMENT" and "NOTHING CONTAINED IN THESE DOCUMENTS
SHALL BE DEEMED AS GRANTING YOU ANY KIND OF LICENSE IN ITS CONTENT" [R15]. The 2002 service templates
say Forum members have licences under the Membership Agreement and "THE UPNP FORUM TAKES NO POSITION
AS TO WHETHER ANY INTELLECTUAL PROPERTY RIGHTS EXIST IN THE STANDARDIZED DCPS" [R11]. Inference (not
legal advice): implementing an interoperable protocol from a publicly downloadable spec is what every
open renderer (pupnp, ohNet, gmrender) does; no royalty programme for UPnP AV was found. The OCF page
ties use of UPnP branding to certification [R15].

DLNA status: the DLNA announced on 5 January 2017 that it would "dissolve as a non-profit trade
association"; "Its certification program continues to be conducted by SpireSpark International" [R16].
DLNA's DRM is DTCP-IP "link protection" [R16], which chorus never implements (K60). Naming
recommendation: call the feature a **"UPnP AV media renderer"** and describe compatibility factually
("works with UPnP/DLNA control apps such as ..."); never use the DLNA logo or "DLNA Certified", and do
not use the UPnP logo (certification-gated [R15]). Trademark conclusion: inference, not legal advice.

### 1.2 OpenHome (Linn's UPnP extension)

- Licences (read from licence files only): ohNet (UPnP stack, C++) "licensed under the MIT license";
  ohPipeline (Linn's media pipeline) MIT; ohSongcast "2-clause (Simplified) BSD" [R17]. All permissive:
  source may be read and cited under K33.
- Services (docs.openhome.org did not resolve, so the list is **ASSUMED**): Product, Volume, Info,
  Time, Playlist, Radio, Receiver, Sender (Songcast), Credentials, Pins. upmpdcli's page confirms it
  implements "the OpenHome ohMedia services" including playlist, radio and Songcast Receiver/Sender
  [R18].
- Why it matters: with plain AVTransport the **control point owns the queue** and feeds the next URI;
  if the phone app is killed, playback stops after the current (and prefetched) track (inference from
  [R11]). OpenHome Playlist puts the queue on the renderer, so the phone can sleep. Kazoo, BubbleUPnP
  and upplay drive OpenHome renderers [R18][R19].
- Multiroom: Songcast is Linn's sender/receiver multiroom stream. chorus has its own sync protocol, so
  Songcast is useful only as an **input** (a PC or phone app sending system audio to a room).
- K64 check: an OpenHome Playlist is a queue of URIs pushed by the control point, not a library or
  directory, so it stays "inputs only" (inference).
- Recommendation: AVTransport renderer first; OpenHome Product/Volume/Info/Time/Playlist as a later
  goal; Songcast receiver optional (owner's call).

### 1.3 Implementations to learn from (licences checked)

| Project | Licence | Use | Source |
|---|---|---|---|
| pupnp (libupnp, C) | BSD-3-Clause | Read/cite; mature UDA stack | [R20] |
| ohNet / ohPipeline (C++) | MIT | Read/cite; OpenHome services, device stack | [R17] |
| ohSongcast | BSD-2-Clause | Songcast receiver reference | [R17] |
| `dlna-dmr` crate 0.1.3 (Rust, axum/tokio/quick-xml) | MIT | Small DMR framework, 0 reverse deps; read, probably not depend | [R21][R22] |
| `rupnp` 3.0.0 / `ssdp-client` 2.1.0 (Rust) | MIT/Apache-2.0 | Control-point side: use in chorus's **tests** to drive the renderer | [R21] |
| `cotton-ssdp` 0.1.0 | CC0-1.0 | SSDP reference | [R21] |
| `tokio-ssdp` 0.1.0 | MIT | Stale (2021) | [R21] |
| `crab-dlna` 0.2.1 | MIT OR Apache-2.0 | A DLNA sender: a second test control point | [R21][R17] |
| `switchy_upnp` (MoosicBox) | MPL-2.0 | Avoid as a dependency (MPL); docs fine | [R21] |
| gmrender-resurrect | **GPL-2.0**: never open source | Avoid | [R20] |
| upmpdcli | **GPL** ("licensed under the GPL"): never open source | Avoid; its docs were read | [R18] |
| MPD | **GPL-2.0** | Avoid | [R20] |
| Rygel / GUPnP, Platinum | LGPL / GPL-or-commercial (**ASSUMED**) | Avoid | ASSUMED |

Recommendation: **write the renderer in chorus-server (Rust)**, from the OCF specs, with pupnp/ohNet as
permissive references and rupnp + crab-dlna as automated test control points. SSDP, a description
document, three SOAP services and GENA are small (size ASSUMED at 2k to 4k lines) and chorus already
hand-writes mDNS (K48), so a dependency buys little; this also keeps K58's "receivers inside chorus"
answer simple for UPnP.

### 1.4 Control points (compatibility targets)

| Control point | Status | Source |
|---|---|---|
| BubbleUPnP (Android) | Plays to "UPnP/DLNA/OpenHome music streamers"; gapless "on some UPnP/DLNA devices"; streams Qobuz and TIDAL; transcodes for renderers (audio) | [R19] |
| Linn Kazoo, upplay | Named as control points for upmpdcli's OpenHome renderer | [R18] |
| Windows "Cast to device" (Play To) | Pushes to DMRs: **ASSUMED** | ASSUMED |
| foobar2000 (UPnP output component), Jellyfin (DLNA plugin, Play To), Plex (DLNA casting support uncertain) | **ASSUMED**, verify in a goal | ASSUMED |

Consequence worth telling the owner: through BubbleUPnP (or similar), TIDAL and Qobuz reach chorus as a
UPnP renderer with no partner status (the control point fetches with the user's own login) [R19].

### 1.5 One renderer per target (K59) and UDN stability

- **One UPnP root device per cast target**: each room, each saved group, each live group. Each has its
  own UDN, friendlyName, description URL (one HTTP listener with per-target path prefixes, e.g.
  `/upnp/<target-id>/desc.xml`, is enough; inference) and its own AVTransport InstanceID 0.
- **UDN** = UUIDv5(namespace = a per-install random UUID stored in server state, name = target key).
  Room key = the room's stable id; saved group key = the saved group's id; live group key = the
  **sorted member-room id set**, so re-forming "Kitchen + Den" gets the same UDN and control points
  that remember devices by UDN (**ASSUMED** behaviour of BubbleUPnP and others) keep working.
- Names: "Kitchen", "Downstairs" (saved group), "Kitchen + Den" (live group). Renames change the
  friendlyName, never the UDN.
- Lifecycle: a live group sends ssdp:alive when formed and ssdp:byebye when dissolved; NOTIFY
  re-announced before max-age expires (e.g. 1800 s; ASSUMED UDA value).
- Count for 8 rooms (K75): 8 room devices + saved groups (say 2 to 4) + at most 4 disjoint live groups
  = at most about 16 devices.
- K78 ("take the room"): a Play on a group's renderer moves each member room into that group; the
  displaced rooms' renderers event TransportState = STOPPED via LastChange so their control points
  see it.

---

## 2. K57: other cast/push protocols under K60

| Protocol | Open without partner status? | Facts | Verdict |
|---|---|---|---|
| Spotify Connect via Soloist | Yes (official software, owner's API key) | Section 3 | **Yes** (K66) |
| AirPlay / AirPlay 2 | No | Hardware receivers come through Apple's MFi programme (**ASSUMED**: developer.apple.com/airplay rendered only a title [R23]). Open receivers exist because "On April 8, 2011, James Laird reverse-engineered and released the private key used by the Apple AirPort Express" [R24]; AirPlay 2 pairing in open receivers is reverse-engineered (ASSUMED, prior research S52). | **No** (K60) |
| Google Cast | No | Public SDK covers sender and receiver *apps*; built-in device Cast for manufacturers ("Google Cast for audio", LG, Sony) [R25]; device authentication certificates are Google-issued (**ASSUMED**, prior research S59). | **No** (K57/K60) |
| Roon Ready (RAAT) | No (**ASSUMED**: roon.app pages fetched had no programme detail [R26]) | Roon Bridge is Roon's own free endpoint software (ASSUMED), so it would pass K60 like Soloist, but only matters if the owner subscribes to Roon. | **No for now**; revisit only if the owner uses Roon |
| Tidal Connect, Qobuz Connect | No (partner-only per K57; developer pages fetched had no public receiver SDK [R27]; ASSUMED) | Covered indirectly: BubbleUPnP streams TIDAL/Qobuz to UPnP renderers [R19]. | **No**; UPnP covers them |
| Deezer | No public receiver SDK found (**ASSUMED**, nothing fetched) | Deezer app casting to UPnP: ASSUMED unknown | **No** |
| Snapcast | Not a cast protocol: a server-to-speaker protocol (Hello, Time, Codec Header, Wire Chunk, Server Settings, Client Info, Error); the doc states no licence of its own; the project is GPL (K26/K33) | [R28] | **No** (K55 chorus-only groups; chorus has its own protocol) |
| MPD protocol | Open text protocol on TCP 6600 with queue, database, outputs and partitions ("separate queue, player and outputs") | [R29]; MPD itself GPL-2.0 [R20] | **No** as a cast target. As a control surface it expects a music database, which K64 excludes; chorusctl, HA and the PWA already cover control. Low-value optional facade only. |
| OpenHome Songcast | Yes (Linn's protocol; ohSongcast BSD-2 [R17]) | Sender apps exist for desktop (ASSUMED current availability) | **Optional** extra: "PC audio to a room". Owner decides. |
| Bluetooth A2DP | Declined (K53) | | **No** |

---

## 3. K63/K66: Spotify Soloist

### 3.1 Facts (all from Spotify's public developer pages)

- What: "a command-line Spotify headless client" for "Raspberry Pis and home servers" [R1]; "a headless
  Spotify client for Linux-based systems" [R30]. Primary test device: Raspberry Pi 3 Model A+ [R2],
  which has "512MB LPDDR2 SDRAM" and a 1.4 GHz Cortex-A53 [R31].
- Platforms: Linux arm64, arm32, x86_64; needs "Working PipeWire or PulseAudio output" [R2]; "When
  PipeWire is available, Spotify Soloist uses PipeWire. If PipeWire cannot initialize, Spotify Soloist
  falls back to PulseAudio" and it routes to the default output unless `--pipewire-device` is set [R8].
- Flags: `-n/--device-name`, `-k/--api-key`, `-D/--data-dir`, `-C/--cache-dir`, `-z/--cache-size`,
  `-d/--pipewire-device` ("Route audio to a specific PipeWire node name or ID"), `-i/--initial-volume`,
  `-s/--single-track URI` ("Play one Spotify URI and exit when done"), `-p/--pair`, `-w/--ws ADDR:PORT`.
  Exit code 10 = "Spotify Soloist build expired" [R4].
- Local WebSocket API [R3]: commands `get_auth_state`, `get_state`, `get_queue`, **`play` with optional
  `uri`** (track/album/playlist/episode), pause, skip_next/skip_prev, seek (`position_ms`), set_volume
  (0 to 100), add_to_queue, set_shuffle, set_repeat_*, **activate/deactivate**. Events: `auth_state`,
  `playback_state` (status idle/playing/paused/buffering, item, context, position, volume, is_active),
  `track_changed`, `playback_changed`, `volume_changed`, `device_changed`, `context_changed`,
  `options_changed`, **`position_sync`** (position_ms + timestamp_ms + speed), `queue_changed` (capped at
  10). Items carry name, creators, parent album, `duration_ms` and **cover image URLs** (small, default,
  large, xlarge). Control commands need `logged_in: true`. Port `0` picks a free port and the server
  writes `ws.addr`/`ws.port` into the data directory. "no built-in client authentication,
  authorization, TLS, Origin validation, CSRF protection" [R3][R5].
- `soloist ctl play [uri]` "starts playing a Spotify URI (track, episode, album, playlist)" [R6].
- Pairing: "authenticates through Spotify Connect by having a user select the device in the Spotify app
  on the same local network"; the session is stored in the data directory; `--pair` re-pairs [R7].
- API key: "The account that generates the key must have Spotify Premium"; "The key is for the person
  who generated it"; "Do not share it with other users, publish it, embed it in client-side
  applications, or include it in distributed scripts" [R7].
- Listeners: "A Premium account is needed to set up Spotify Soloist, but once it's running both Free
  and Premium users can connect"; lossless (up to 24-bit/44.1 kHz) and crossfade/automix are Premium
  [R1].
- Builds: "expire 90 days after their build date"; update = "Download the archive for the same
  architecture, replace the installed `soloist` executable, and restart the daemon"; `soloist --version`
  prints the build timestamp; no manifest, "latest" URL or checksums are published; "Do not
  redistribute Spotify Soloist archives or binaries directly" [R9].
- Terms: the key is generated after accepting Soloist Terms and Conditions in the dashboard (behind
  login: **NOT FETCHED**) [R2]. Developer Terms v10 (15 May 2025) grant rights "for private personal
  use", forbid "reverse-engineering", and say "you may not use more than one Security Code for each
  SDA" [R10]; whether a Soloist instance is an "SDA" is not stated (inference: ambiguous).
- **Not stated anywhere fetched**: an instance limit per key, account or household; container support;
  commercial use; CPU/memory; output sample format. Prior research recorded Music Assistant's note that
  "Spotify's terms do not clearly allow using Soloist this way" (per-player instances) [R32].
- Spotify-side constraint: one account plays on one Connect device at a time (the WebSocket reference's
  activate/deactivate model [R3]; general Spotify behaviour ASSUMED). Different household accounts can
  use different rooms at once.

### 3.2 Proposed sidecar design

| Aspect | Proposal |
|---|---|
| Container | One `chorus-soloist` container (not one per target): a small image the owner builds locally with the Soloist binary **mounted from a host volume** (chorus never ships or downloads it; no redistribution [R9]). It runs PipeWire + WirePlumber (both MIT [R33]) headless as the container's only audio server, plus one Soloist process per target, supervised by a small chorus binary `chorus-soloistd` (Rust, MIT OR Apache-2.0). Host network (Connect discovery on the LAN; ASSUMED mDNS/zeroconf), so it joins K34's host-network exception in the homelab PR. |
| Why not one container per target | Up to 16 targets would mean 16 PipeWire daemons and 16 compose services that must appear/disappear with live groups; a supervisor can start and stop processes in milliseconds (inference). |
| Audio path | Per target, a PipeWire **pipe-tunnel sink** (`tunnel.mode = sink`, `pipe.filename = /run/chorus/soloist/<target>.pcm`, fixed S16LE or F32 stereo 44.1 kHz, `node.name = chorus-<target>`): "Samples played on the sink will be written to the pipe" [R34]. Soloist runs with `--pipewire-device chorus-<target>` [R4]. chorus-server reads the FIFO as an ordinary FIFO input (its existing input type), timestamps on arrival and resamples into the group's timeline. Alternative: a native PipeWire client in chorus via a PipeWire binding crate (licence ASSUMED MIT) avoids FIFOs; pick FIFO first (simpler, and PipeWire's pacing of a FIFO sink needs a measurement; ASSUMED). |
| Naming | `--device-name` = the target's display name ("Kitchen", "Downstairs", "Kitchen + Den"); data dir `/var/lib/chorus/soloist/<target-key>` where the key is the same as the UPnP UDN key (room id, saved group id, sorted member set), so a re-formed live group keeps its pairing. |
| Control and metadata | Each Soloist runs `--ws 127.0.0.1:0`; `chorus-soloistd` reads `ws.port` from the data dir and relays `playback_state`/`position_sync`/`track_changed` to chorus-server (K65 metadata + artwork URLs + position), and relays chorus commands (pause, volume, `play uri`). Loopback only, because the API has no authentication [R3]. |
| Volume | Soloist volume events map to the target's chorus volume (K77 group scaling); Soloist's own volume stays at 100 so chorus's DSP applies volume once (inference). |
| Rooms and saved groups | Instances always running (visible in the Spotify app). |
| Live groups (K59) | Start an instance when the group forms, stop it after a grace period (e.g. 60 s) when it dissolves and is idle. A live group that forms for the first time needs one pairing (select it in the Spotify app) [R7]. |
| Take the room (K78) | When a group instance reports `playing`, chorus moves member rooms into the group; each displaced room's own instance gets `pause` (and `deactivate` if it was active) so the Spotify app shows the truth. |
| Alarms (K80) | `play` with `uri` on the room's instance (requires it paired and `logged_in`) [R3]; plays in the paired account's context. `--single-track` is a fallback for a single track [R4]. Alarm fallback when the instance is expired or logged out: a chime (K80's built-in chime). |
| Build expiry | `chorus-soloistd` reads the build timestamp from `soloist --version` [R9], raises "Soloist build expires in N days" at T-14 in the app and HA (a Needs Owner line), and treats exit 10 as "expired" (no restart loop). Update is an owner step: download from Spotify's page, replace the mounted binary, `chorusctl soloist restart`. |
| Terms follow-up (owner step) | When generating the key, the owner reads the dashboard T&C for (a) instance count per key, (b) household/multi-account use, (c) personal-use scope, and records it in `docs/decisions/`. If the T&C cap instances, fall back to rooms-only instances (plus a "move to group" action in the app). |

---

## 4. K79: decoders

### 4.1 Where compressed audio enters chorus

- UPnP renders: whatever the control point sends (MP3, FLAC, AAC/M4A, ALAC, Ogg, WAV); the renderer's
  GetProtocolInfo Sink list decides what control points offer or transcode [R14][R19].
- **HA TTS/announcements: default format is MP3** (`_DEFAULT_FORMAT = "mp3"` in HA core
  `homeassistant/components/tts/__init__.py`, Apache-2.0) and HA converts with ffmpeg when a
  `preferred_format` is requested [R35][R36]. The chorus HA integration can request `flac` or `wav` for
  its own announce path (inference from [R35]).
- Alarm stream URLs (internet radio): MP3 and AAC/HE-AAC over HTTP (Icecast/Shoutcast; share of each
  ASSUMED), Ogg Vorbis/Opus, and HLS, whose segments are MPEG-2 TS, fragmented MP4 or packed audio
  ("AAC with ADTS framing, MP3, AC-3, and Enhanced AC-3"); RFC 8216 is an Informational, Independent
  Submission (August 2017) [R37].
- Soloist delivers PCM through PipeWire (no decoder in chorus).

### 4.2 Per format (US, 2026)

| Format | Patent status (US) | Permissive decoders (licence) | Recommendation |
|---|---|---|---|
| WAV / LPCM | None (ASSUMED) | own parser; hound (Apache-2.0); dr_wav (Unlicense or MIT-0) [R21][R38] | Symphonia `wav`/`pcm` or own |
| MP3 | Expired: "April 16, 2017 ... U.S. patent 6,009,399 ... expired", after which "the MP3 technology became patent-free in the United States"; Sisvel's programme "has become a legacy" (2023) [R39] | minimp3 (CC0-1.0) [R20]; dr_mp3 (Unlicense/MIT-0, based on minimp3) [R38]; nanomp3 (MIT OR Apache-2.0, pure Rust from minimp3); puremp3 (MIT OR CC0, 2019); Symphonia `mp3` (MPL-2.0, "Excellent", gapless yes, not default-on) [R21][R40] | **Yes**, Symphonia |
| FLAC | Royalty-free (Xiph; no known patents: ASSUMED) | libFLAC (BSD-3) [R41]; claxon (Apache-2.0, last release 2020); Symphonia `flac` (Excellent, gapless); dr_flac (Unlicense/MIT-0) for C [R21][R38][R40] | **Yes**: Symphonia (server), dr_flac (endpoint, K62) |
| Opus | Royalty-free patent licences listed in libopus COPYING (Xiph, Microsoft, Broadcom IETF IPR declarations) [R41] | libopus (BSD-3) [R41]; bindings `opus` (MIT/Apache-2.0), audiopus (ISC), opusic-sys (BSD-3); pure Rust `ropus` (BSD-3, 0.12.x), `opus-decoder` (MIT OR Apache-2.0, 0.1.x, young); Symphonia Opus not released ("-") [R21][R40] | **Yes**: libopus via a MIT/Apache binding (also needed to **encode** for the wire, K62); libopus fixed-point on the endpoint |
| Vorbis | Royalty-free (Xiph; ASSUMED) | libvorbis (BSD-3) [R20]; Tremor (BSD, ASSUMED); stb_vorbis (MIT or public domain) [R41]; lewton (MIT OR Apache-2.0, last release 2021); Symphonia `vorbis` (Excellent, gapless) [R21][R40] | **Yes**, Symphonia |
| ALAC | Apple's reference decoder is Apache-2.0 (macosforge/alac, archived) [R20]; Apache-2.0 includes a contributor patent grant (clause content ASSUMED) | Apple ALAC (Apache-2.0); `alac` crate (MIT/Apache-2.0, 2018); Symphonia `alac` (Great, gapless) + `isomp4` [R21][R40] | **Yes**, Symphonia |
| AAC-LC | **Live**: "the last baseline AAC patent expires in 2028, and the last patent for all AAC extensions mentioned expires in 2031" (Wikipedia citing SEC terms) [R42]. Via LA: "An AAC patent license is needed by manufacturers or developers of end-user encoder and/or decoder products"; "License fees are due on the sale of encoders and/or decoders only"; no fees for distributing AAC bitstreams; tiered per-unit fees from $0.98 (1 to 500,000 units) [R43] | Symphonia `aac` (MPL-2.0, AAC-LC "Great", no gapless, **default off**) [R40]; fdk-aac: Fraunhofer licence, "NO EXPRESS OR IMPLIED LICENSES TO ANY PATENT CLAIMS ... ARE GRANTED", use "only for purposes that are authorized by appropriate patent licenses", and binary redistribution must ship source [R44]; `fdk-aac` crate is a MIT binding over that library [R21]; faad2: **GPL, avoid** | **Owner decides** (see 4.3). Recommended: off by default, behind a build feature |
| HE-AAC v1/v2 (SBR/PS) | Covered by the same Via LA programme (AAC-LC, HE-AAC, HE-AAC v2, xHE-AAC) [R43]; extensions until about 2031 [R42] | Symphonia lists `he-aac`/`he-aac-v2` with **no status** (not implemented) [R40]; fdk-aac implements it (licence above) | Same decision as AAC; without SBR an HE-AAC stream decodes as its AAC-LC core at reduced bandwidth (ASSUMED property of HE-AAC's backward-compatible signalling) |
| HLS (container/protocol) | No licence required for the playlist format (ASSUMED); segments usually AAC [R37] | own m3u8 + TS/packed-audio demuxer (small) | Later; effectively gated by the AAC decision |

### 4.3 AAC under the owner's strict bar (facts, then options; not legal advice)

Facts: (1) K60 is about service terms and DRM/authentication reverse engineering; AAC decoding
involves neither: it is a **patent** question, a separate axis. (2) US AAC patents are live until
about 2028 (baseline) and 2031 (extensions) [R42]. (3) Via LA licenses "manufacturers or developers
of end-user ... decoder products" and charges on sales [R43]. (4) US law: "whoever without authority
makes, uses, offers to sell, or sells any patented invention, within the United States ... infringes
the patent" (35 U.S.C. 271(a)) [R45]: the statute text names **use**, not only sale, and has no
private-use carve-out in that subsection. (5) Common OSS practice is to ship AAC off by default
(Symphonia's default feature set "only enables support royalty-free open standard codecs" [R40]) or to
assign patent responsibility to users (FFmpeg, per [R42]).

Options: (A) **No AAC until the baseline patents expire** (about 2028): MP3/FLAC/Opus/Vorbis/ALAC/PCM
only; the UPnP Sink list omits `audio/mp4`/`audio/aac`, so control points like BubbleUPnP transcode
(it transcodes audio for UPnP renderers [R19]); AAC radio URLs and HLS fail with a clear error.
(B) **AAC-LC behind a default-off build feature** (`chorus-server --features aac` enabling Symphonia
`aac`), with a written notice; the owner decides whether his private build turns it on. (C) Move
decoding to an external tool (an ffmpeg sidecar): changes the licence surface (LGPL, separate
process), not the patent question. **Recommendation: B, shipped with the feature off (which is A in
effect) until the owner decides at Checkpoint K**; confidence medium, because it is a legal-risk call
that belongs to the owner.

### 4.4 Implementation shape and K26 compatibility

- **Server (Rust)**: one `chorus-decode` crate wrapping Symphonia 0.6 (features `mp3`, `flac`,
  `vorbis`, `alac`, `pcm`, `wav`, `ogg`, `isomp4`; `aac` behind chorus's own default-off feature) plus
  libopus through the `opus` crate (MIT/Apache-2.0 binding, BSD-3 C library) for Opus decode and wire
  encode. Crates, not vendored (lockfile-pinned; K18 pinning rule). Symphonia MSRV 1.85 [R40].
- **MPL-2.0 and K26**: chorus's own code stays MIT OR Apache-2.0. MPL-2.0 is file-level copyleft
  (ASSUMED summary of MPL-2.0): using unmodified Symphonia as a dependency imposes no licence on
  chorus's files; distributing binaries requires making the MPL-covered source available (crates.io
  source plus a notice suffices: ASSUMED). Record it in an ADR and allow `MPL-2.0` in cargo-deny for
  `symphonia*` only. All-permissive alternative: nanomp3 + claxon + lewton + alac + hound (every one MIT
  and/or Apache-2.0 or CC0) at the cost of four APIs, older releases (2018 to 2021) and no shared
  gapless handling.
- **Endpoint (C, ESP32)**: decodes only what the wire carries (K62: PCM, FLAC, Opus): dr_flac vendored
  (single header, Unlicense or MIT-0) and libopus fixed-point vendored as an ESP-IDF component, both
  pinned by hash; the same fixtures run through the Rust decoders (CLAUDE.md rule 5).
- Gapless across UPnP tracks needs decoder delay/padding trimming (Symphonia's gapless column: MP3,
  FLAC, Vorbis, ALAC yes; AAC no) [R40], plus SetNextAVTransportURI prefetch [R11].

---

## 5. K76: concurrent streams

### 5.1 Measured (this session, 2026-09-29)

Measured in the chorus dev container on the production host class: **Intel Xeon E5-2680 v4 @ 2.40 GHz**
(same host as production per survey-homelab.md, host shared with other load, so figures are upper
bounds). Tool: ffmpeg N-125875 (2026-07-31) with `-benchmark -threads 1`, decoding a 600 s, 44.1 kHz
stereo test signal (sine + pink noise) to s16le into /dev/null, three runs each (two for encodes).
ffmpeg is a proxy for Symphonia, which reports "+/-15% the performance of FFMpeg" [R40]. Raw
output is in this session's scratchpad only; a goal should repeat this with chorus's own decoders and
file it in docs/measurements/ labelled "host".

| Stream (600 s audio) | CPU s (user+sys, median) | Share of one core | Process max RSS |
|---|---|---|---|
| MP3 320 kbit/s decode | 2.07 | 0.35% | 33 MB (whole ffmpeg process) |
| AAC-LC 256 kbit/s decode | 2.08 | 0.35% | 33 MB |
| FLAC decode | 0.95 | 0.16% | 33 MB |
| Opus 128 kbit/s decode | 3.13 | 0.52% | 33 MB |
| Vorbis q6 decode | 1.97 | 0.33% | 33 MB |
| ALAC decode | 2.00 | 0.33% | 33 MB |
| Opus 256 kbit/s **encode** (wire) | 11.5 | 1.9% | 33 MB |
| FLAC level 5 **encode** (wire) | 1.34 | 0.22% | 33 MB |
| Resample 44.1 to 48 kHz | 1.06 | 0.18% | 33 MB |

(Rounded from runs: MP3 2.25/2.07/2.03; AAC 1.80/2.19/2.08; FLAC 0.89/0.95/1.07; Opus 3.01/3.27/3.13;
Vorbis 1.92/2.08/1.97; ALAC 2.00/2.23/1.98; Opus enc 11.94/11.14; FLAC enc 1.32/1.35; resample
0.97/1.14. `utime` exceeded wall time, so ffmpeg used a helper thread despite `-threads 1`.)

### 5.2 Estimates (ASSUMED where marked)

- Per independent stream in chorus-server: decode (at most 0.6%) + resample (0.2%) + DSP (K30 EQ,
  loudness: ASSUMED at most 1%) + one wire encode per distinct stream (Opus 1.9% or FLAC 0.2%) + per-
  speaker encryption (ASSUMED small): about **3% of one core** worst case. 8 streams: about 25% of one
  core, well inside a `cpus: 2` quota. Memory per decode stream: decoder state plus a few seconds of
  buffer, ASSUMED under 5 MB.
- Per Soloist instance: **no published figures**. It is tested on a 512 MB Raspberry Pi 3 A+ [R2][R31],
  so ASSUMED 50 to 100 MB RSS and 1% to 3% of a Broadwell core while playing, near zero idle.
  PipeWire + WirePlumber: ASSUMED 20 to 40 MB total.
- 8-room house: at most 8 independent streams (every room separate), at most 16 Soloist instances
  (8 rooms + about 4 saved + about 4 live), at most about 16 UPnP devices.

### 5.3 Proposed limits

| Limit | Value | Why |
|---|---|---|
| Independent streams (server) | **8** (= rooms, K75), configurable | CPU is not the constraint (section 5.1); a stream per room is the natural maximum |
| Soloist instances | **16**, configurable; container `mem_limit: 2g`, `cpus: 2` | ASSUMED 100 MB each worst case; replace with a measurement (owner step: needs the API key) |
| UPnP renderer devices | one per target (about 16) | Cost is SSDP traffic and idle sockets only |

What would change it: a measured Soloist RSS above 150 MB, the Soloist T&C capping instances, or
DSP (room correction FIR, K31) costing far more than ASSUMED.

---

## 6. Decision table

| Decision | Options | Recommendation | Confidence | What would change it |
|---|---|---|---|---|
| K57 UPnP renderer form | Write in chorus-server (Rust) / depend on `dlna-dmr` / wrap pupnp (C, BSD-3) / GPL renderer sidecar (excluded) | **Write in chorus-server** from OCF specs; test with rupnp + crab-dlna; call it "UPnP AV media renderer" | High | A mature MIT/Apache Rust DMR crate appearing; control-point interop failures that pupnp handles |
| K57 OpenHome | None / Playlist-Product-Volume-Info-Time later / plus Songcast receiver | **AVTransport first; OpenHome services in a later goal**; Songcast receiver optional | Medium | Owner's control-point choice (Kazoo/BubbleUPnP OpenHome users); phone-sleep complaints |
| K57 other protocols | AirPlay, Cast, Roon Ready, Tidal/Qobuz Connect, Deezer, Snapcast, MPD, Songcast | **Add none now**; Songcast optional; Roon Bridge only if the owner uses Roon | High (partner-only items ASSUMED) | A published open receiver SDK from a service; the owner subscribing to Roon |
| K59 target advertising | One device per target / one device with many instances | **One root device per target, UUIDv5 UDN from install namespace + target key (live group = sorted members)** | High | Control points mis-handling many devices from one IP (test with BubbleUPnP) |
| K58 where receivers run | In chorus-server / sidecars | **UPnP in chorus-server; Soloist in one `chorus-soloist` sidecar** with a supervisor | High | Soloist T&C forbidding multiple instances per host |
| K66 Soloist topology | Container per target / one container, process per target | **One container, PipeWire pipe-tunnel sink per target, FIFO into chorus-server, loopback WS** | Medium | PipeWire FIFO pacing problems (then a native PipeWire client); instance caps |
| K80 Spotify alarms | WS `play uri` / `--single-track` / none | **WS `play` with `uri` on the room's instance; chime fallback** | High (documented) | The instance being logged out or expired at alarm time |
| K79 decoders (server) | Symphonia (MPL-2.0) / all-permissive crate set / vendored C (dr_libs, minimp3, stb_vorbis) | **Symphonia + libopus binding** | Medium-high | Owner or K26 reading forbidding MPL-2.0 dependencies (then the permissive crate set) |
| K79 decoders (endpoint) | dr_flac + libopus / own decoders | **Vendored dr_flac + libopus (fixed-point)** | High | K62 wire codecs changing |
| K79 AAC / HE-AAC | Off until about 2028 / default-off build feature / on / ffmpeg sidecar | **Default-off feature (Symphonia AAC-LC); owner decides**; no HE-AAC decoder exists permissively | Medium (legal call, not legal advice) | Owner's risk decision; baseline patent expiry (about 2028) |
| K79 HLS | Now / later / never | **Later**, after the AAC decision | Medium | Owner's alarm stations being HLS-only |
| K76 stream limit | 1 per room / up to 4 / measured | **8 independent streams, 16 Soloist instances, 2 GB Soloist budget** | Medium (Soloist ASSUMED) | Measured Soloist RSS/CPU; room-correction DSP cost |

---

## 7. What was read (all 2026-09-29)

GPL projects: only licence metadata, READMEs or docs pages were read (upmpdcli homepage, Snapcast
binary_protocol.md, MPD protocol docs, GitHub licence API for gmrender-resurrect and MPD). No GPL
source file was opened (K33). Permissive files read: licence files of ohNet, ohPipeline, ohSongcast,
fdk-aac NOTICE, libopus COPYING, stb LICENSE, libFLAC COPYING.Xiph, crab-dlna LICENSE-MIT, dr_flac.h
licence footer, PipeWire COPYING/LICENSE, HA core `tts/__init__.py` (Apache-2.0, grepped for format).

- R1 https://developer.spotify.com/blog/2026-08-13-introducing-spotify-soloist
- R2 https://developer.spotify.com/documentation/soloist/tutorials/getting-started
- R3 https://developer.spotify.com/documentation/soloist/reference/websocket-api
- R4 https://developer.spotify.com/documentation/soloist/reference/command-line
- R5 https://developer.spotify.com/documentation/soloist/features (and /howtos/basic-integration)
- R6 https://developer.spotify.com/documentation/soloist/reference/soloist-ctl
- R7 https://developer.spotify.com/documentation/soloist/concepts/authentication
- R8 https://developer.spotify.com/documentation/soloist/concepts/overview
- R9 https://developer.spotify.com/documentation/soloist/reference/downloads-and-updates
- R10 https://developer.spotify.com/terms (Developer Terms v10, effective 15 May 2025)
- R11 https://upnp.org/specs/av/UPnP-av-AVTransport-v1-Service.pdf (text extracted with pypdf)
- R12 https://upnp.org/specs/av/UPnP-av-MediaRenderer-v1-Device.pdf
- R13 https://upnp.org/specs/av/UPnP-av-RenderingControl-v1-Service.pdf
- R14 https://upnp.org/specs/av/UPnP-av-ConnectionManager-v1-Service.pdf
- R15 https://openconnectivity.org/developer/specifications/upnp-resources/upnp/
- R16 https://en.wikipedia.org/wiki/Digital_Living_Network_Alliance
- R17 Licence files: raw.githubusercontent.com openhome/ohNet/License.txt, openhome/ohPipeline/License.txt,
  openhome/ohSongcast/License.txt, gabrielmagno/crab-dlna/LICENSE-MIT
- R18 https://www.lesbonscomptes.com/upmpdcli/ (homepage only)
- R19 https://bubblesoftapps.com/bubbleupnp/
- R20 GitHub API repo licences: pupnp/pupnp (BSD-3-Clause), hzeller/gmrender-resurrect (GPL-2.0),
  MusicPlayerDaemon/MPD (GPL-2.0), macosforge/alac (Apache-2.0, archived), lieff/minimp3 (CC0-1.0),
  xiph/vorbis (BSD-3-Clause), plus NOASSERTION results followed up in R17/R38/R41/R44
- R21 crates.io API (versions, licences, dates): rupnp, ssdp-client, dlna-dmr, cotton-ssdp, tokio-ssdp,
  switchy_upnp, crab-dlna, symphonia, symphonia-codec-aac, claxon, lewton, minimp3, nanomp3, puremp3,
  rmp3, audiopus, opus, opusic-sys, ropus, opus-decoder, unsafe-libopus, ogg, alac, hound, fdk-aac,
  mp3lame-sys (LGPL-3.0, avoid)
- R22 https://docs.rs/dlna-dmr/latest/dlna_dmr/
- R23 https://developer.apple.com/airplay/ (rendered only a title; nothing usable)
- R24 https://en.wikipedia.org/wiki/AirPlay
- R25 https://en.wikipedia.org/wiki/Google_Cast (and https://developers.google.com/cast/docs/overview)
- R26 https://roon.app/en/partners (no programme detail); roon.app/en/roon-ready and Wikipedia Roon: 404
- R27 https://developer.tidal.com/ (rendered only a title); qobuz.com Qobuz Connect page: 404
- R28 https://github.com/badaix/snapcast/blob/develop/doc/binary_protocol.md (doc only)
- R29 https://mpd.readthedocs.io/en/latest/protocol.html
- R30 https://developer.spotify.com/documentation/soloist
- R31 https://www.raspberrypi.com/products/raspberry-pi-3-model-a-plus/
- R32 /cache/tmp/plan-2026-09-chorus/research-ha-ma-sources.md, row "MA's Spotify Connect plugin" citing
  https://www.music-assistant.io/plugins/spotify-connect/ (read earlier today by another session)
- R33 https://raw.githubusercontent.com/PipeWire/pipewire/master/COPYING and LICENSE (MIT, with LGPL
  alsa plugin and GPL jackserver exceptions); GitHub API PipeWire/wireplumber (MIT)
- R34 https://docs.pipewire.org/page_module_pipe_tunnel.html
- R35 https://raw.githubusercontent.com/home-assistant/core/dev/homeassistant/components/tts/__init__.py
  (line 107 `_DEFAULT_FORMAT = "mp3"`; Apache-2.0 per the repo LICENSE.md)
- R36 https://www.home-assistant.io/integrations/tts/ (and https://developers.home-assistant.io/docs/core/entity/tts/)
- R37 https://www.rfc-editor.org/rfc/rfc8216.txt (sections 3.1 to 3.4)
- R38 https://raw.githubusercontent.com/mackron/dr_libs/master/README.md and dr_flac.h licence footer
- R39 https://en.wikipedia.org/wiki/MP3 (licensing section)
- R40 https://github.com/pdeljanov/Symphonia and raw README.md (codec/format tables, licence, MSRV)
- R41 https://raw.githubusercontent.com/xiph/opus/main/COPYING, nothings/stb/LICENSE,
  xiph/flac/COPYING.Xiph
- R42 https://en.wikipedia.org/wiki/Advanced_Audio_Coding (licensing and patents section)
- R43 https://www.via-la.com/licensing-programs/aac/
- R44 https://raw.githubusercontent.com/mstorsjo/fdk-aac/master/NOTICE
- R45 https://www.law.cornell.edu/uscode/text/35/271
- Not reachable: docs.openhome.org (DNS failure), developer.spotify.com/documentation/soloist/faq (404),
  gitlab.freedesktop.org (bot wall), Soloist T&C (dashboard, login).
- Local measurement: ffmpeg on the chorus dev container (host CPU Xeon E5-2680 v4), section 5.1.
