> **Declined by the owner (decision K56, 2026-09-29): no Music Assistant and no Sendspin.** This study's recommendation is not an option. Read it only for its feature list (§1.1), voice (§3), MQTT (§4) and security (§5) facts.

# Research: HA / Music Assistant path, music sources, voice, MQTT (K14, K15, K31, K46, K40)

Written 2026-09-29 for the chorus /goal program plan. Read-only research: no repo changed.
Every source below was read on 2026-09-29; inline tags `[S#]` point to the source list in section 7
(URL + "read 2026-09-29"). Claims from memory are marked **ASSUMED**. K33 held: no GPL source
file was opened (section 7 lists what was read and what was deliberately not read). Permissive
source read: Music Assistant's Sendspin provider (Apache-2.0) and Home Assistant core (Apache-2.0).

---

## 0. Headline

1. **Sendspin is now a 1.0 release candidate, not the "technical preview" K15 recorded.** Tag
   `1.0.0-rc1` was published 2026-09-17 [S2]. The spec is under the Community Specification License
   1.0 with a royalty-free patent grant [S1][S3]. Its transport is plain `ws://` carrying a
   **Noise KKpsk2** encrypted session with a **pairing** layer [S1]. That is a big core change since
   the 2025 drafts, and aiosendspin went through 9 major versions (9.0.0 on 2026-08-10) [S5].
   Expect churn until 1.0.
2. **A Sendspin client MUST use the Sendspin time-filter** (a 2-D Kalman filter on offset and drift)
   and MUST hold **within ±1 ms** at the audio output in steady state (target ±0.5 ms) [S1]. So
   Sendspin-native endpoints (K15c) would replace chorus's own offset estimator on those endpoints.
   That collides with "the sync engine is the project".
3. **MA can stream to a chorus-server acting as a Sendspin *client* bridge.** This is the pattern
   the MIT-licensed `sendspin-bt-bridge` already uses (one Sendspin player per speaker, built on
   aiosendspin) [S61]. chorus-server would appear to MA as N Sendspin players (one per zone),
   receive MA's timestamped PCM, and re-time it onto chorus's own timeline. chorus keeps its
   protocol, servo and endpoints. MA's own "Sendspin Bridges" are *in-process* roles inside MA
   (`player@_bridge`, 44.1 kHz/16-bit) for AirPlay and Cast. They are not an external API [S17].
4. **MA does not load out-of-tree providers.** The maintainers declined an extension point:
   "No, we don't consider that. Just follow the development workflow and we're open for PR's."
   [S21]. So an MA "chorus provider" means a fork or an upstream PR (K42: upstream is the Needs list).
5. **HACS cannot install from private repositories** ("Private GitHub repositories can not be used
   with HACS at all") [S42]. chorus is private (K8), so a chorus HA custom integration installs by
   copying into `custom_components` (an owner action on the homelab host).
6. **HA 2026.9.3 has no MQTT `media_player` platform.** This is verified in source:
   `SUPPORTED_COMPONENTS` in `homeassistant/components/mqtt/const.py` lists 33 platforms, and
   `media_player` is not one of them [S43][S44]. It also has **no core `sendspin` integration**
   [S43].
7. **HA's Music Assistant integration already gives Sonos-grade `media_player` entities** for
   every MA player. It maps MEDIA_ANNOUNCE, BROWSE_MEDIA, SEARCH_MEDIA, MEDIA_ENQUEUE, and GROUPING
   (when the MA player has SET_MEMBERS) (verified in source) [S43]. With the bridge, chorus zones
   get join/unjoin, browse, queue and announce in HA without chorus writing HA code.
8. **Announcements: MA's "native" Sendspin ducking is really a relay to the device's ESPHome
   entity in HA.** MA enables PLAY_ANNOUNCEMENT only for Sendspin players whose manufacturer is
   "ESPHome", matched by MAC [S17]. A chorus bridge player would fall back to MA's
   pause/announce/resume [S24]. True duck-and-mix needs chorus's own announce path (see K31).
9. **ESPHome's C++ runtime is GPLv3** (the Python side is MIT) [S34]. The ESPHome `sendspin`,
   `voice_assistant` and `micro_wake_word` component sources are therefore off-limits under K33.
   ESPHome-based chorus firmware would also be GPL, which conflicts with K26 (MIT OR Apache-2.0).
   The Apache-2.0 `sendspin-cpp` library is separate and readable [S6].
10. **Music sources: MA already hosts all of it outside chorus's codebase.** That covers
    Spotify/Apple Music/Tidal/Qobuz/YouTube Music/radio/Plex/Jellyfin/local [S26], plus a Spotify
    Connect plugin (official Spotify Soloist engine or GPL go-librespot) [S27] and an AirPlay
    Receiver plugin that exposes *any MA player* as an AirPlay target [S28]. Via the bridge, chorus
    zones inherit these, and BRIEF §2.3 ("no DRM streaming integrations in this codebase") holds in
    letter.

---

## 1. K15: the HA / Music Assistant path

### 1.1 Sendspin facts (all from the spec text [S1] unless tagged)

| Item | Finding |
|---|---|
| Status / version | `1.0.0-rc1`, released 2026-09-17 [S2]. MatterAlpha: "Release Candidate 1 ... 1.0 expected later in 2026", with a certification program for commercial products "in the works" [S11]. MA docs still say "Technical Preview ... expect it to change" [S12]. MA's provider manifest says `"stage": "beta"`, `"builtin": true`, `"allow_disable": false` [S17]. |
| Licence | Spec: Community Specification License 1.0, "royalty-free patent license from every contributor" within SCOPE.md [S1][S3]. "Sendspin" is an Open Home Foundation trademark, and implementing it "grants no right to use the Sendspin name or logo on commercial products" [S1]. Reference libraries are Apache-2.0 (aiosendspin, sendspin-cpp, sendspin-rs, sendspin-go, time-filter, sendspin-js, SendspinKit). sendspin-dotnet is MIT. `conformance` has no licence [S4][S5]. |
| Transport | WebSocket, which "MUST be plain `ws://`". Confidentiality comes from **Noise `KKpsk2`**, with suites `25519_ChaChaPoly_SHA256` and `25519_AESGCM_SHA256`. Servers support both; clients support at least one. The server is always the Noise initiator. Identity = a Curve25519 keypair (the public key is the `client_id`). A long-term 32-byte PSK comes from **pairing** (PSK token, static 8-digit code, or dynamic 6-digit/QR code with PAKE). Optional "unpaired access". |
| Discovery / direction | Server-initiated is RECOMMENDED: clients advertise `_sendspin._tcp.local.` (port 8928, TXT `path=/sendspin`, optional `name`) and the server connects. Client-initiated: servers advertise `_sendspin-server._tcp.local.` (port 8927). "Servers MUST support both methods"; a client uses exactly one. |
| Multi-server | Server-initiated: a client holds one admitted connection, ranked by activity (`playback` > `pairing` > none), and remembers the "last-playback server". Client-initiated: server selection is implementation-defined. MA docs: "Only one Music Assistant server per network can use Sendspin" [S12]. |
| Clock sync | NTP-style four timestamps (`client/time` -> `server/time`), in microseconds, on "the server's monotonic clock". "Clients MUST use the time-filter algorithm", a 2-D Kalman filter tracking offset and drift. The reference is C++ (Apache-2.0) [S8]. Recommended usage: bursts of 8 exchanges every ~10 s, keeping the lowest-`max_error` sample [S8]. A player "MUST NOT report `available: true` until its time filter has converged". |
| Sync rules | "Accuracy floor: In steady state, implementations MUST keep this error within ±1 ms"; "Accuracy target: SHOULD aim for ±0.5 ms". Error is measured against the time-filter's prediction, not the true server clock. Speed deviation stays within ±0.5% over 150 ms. Suggested correction: drop or duplicate single frames with a ~100 µs dead band, plus a one-shot snap for large errors. The shape is close to BRIEF §5.3. |
| Stated accuracy in practice | Project claim only: ~50 µs median between two ESP32-S3 boards on Wi-Fi ("preliminary testing", Aug 2025) [S15]. No independent measurement was found. OHF publishes a `sync-test` tool: two-channel USB capture, chirps, cross-correlation, "~0.1 µs" sub-sample resolution [S9]. |
| Codecs | "Servers MUST support the `flac` and `pcm` codecs and MAY support `opus`." Players MUST list `flac` or `pcm`, so **a PCM-only player is compliant**. PCM is little-endian signed, with 24-bit packed in 3 bytes. Opus carries a third-party patent note. The server resamples for a player that lists a single rate. MA's Sendspin output is 16-bit ("higher resolution material converted") [S12]. |
| Audio chunk | Binary type 4: int64 timestamp (µs, server clock, when the first sample plays) + uint32 `send_ahead` + payload. Chunks are 15-150 ms. The server honours each player's `buffer_capacity` (bytes), `min_buffer_ms`, `required_lead_time_ms` and `output_delay_ms` (0-5000). A group uses a common send-ahead = the maximum across players. |
| Roles | `player@v1`, `source@v1` (a client streams captured audio *to* the server, for example line-in), `controller@v1`, `metadata@v1`, `artwork@v1`, `visualizer@v1`, `color@v1`. "All servers MUST implement all versions of these roles". Custom roles start with `_` (for example `_vendor_role@v1`). |
| Groups | Every client is in exactly one group. A group carries members, volume, mute and `playing`/`stopped`. `group/update` is pushed. Volume is 0-100 perceptual (`amplitude = (v/100)^1.5` suggested). A `switch` command cycles groups. |
| MA as Sendspin server | MA's built-in `sendspin` player provider requires `aiosendspin[server]==9.1.1` and listens on `ws://<ma>:8927/sendspin`. It also offers WebRTC DataChannels for browsers and apps (remote access) [S17]. It is discovered automatically; pairing can be PIN (dynamic), static PIN, or unpaired; static delay is 0-5000 ms per player [S12]. |
| ESPHome | `sendspin` hub + `media_player`/`media_source`/`image`/`sensor`/`text_sensor` platforms. "Experimental. Breaking changes may occur". ESP32 only, with TCP 8928 + mDNS [S31]. Community builds use ESP32-S3 with PSRAM [S35]. The ESPHome C++ runtime is GPLv3 [S34]. It builds on `sendspin-cpp` (Apache-2.0, ESP-IDF >= 5.1, FLAC/PCM/Opus, WebSocket client and server) [S6]. |
| Rust | `sendspin-rs` is Apache-2.0, v0.3.7 (2026-08-21), marked "WIP. Please help!". It is Tokio-based, and its README shows "Phase 2: Audio Pipeline (Next)" [S5][S7]. Its README says "NTP-style" sync, so time-filter conformance is unverified. |

### 1.2 How a third-party server (chorus-server) can interoperate

- **Bridge (chorus-server = N Sendspin clients):** this works within the spec. chorus-server
  advertises one `_sendspin._tcp` instance per chorus zone and accepts MA's connection
  (server-initiated). Each advertisement has its own TXT `path` and identity keypair. Several paths
  on one port is **ASSUMED** allowed, since the spec only requires a `path` per instance. chorus
  lists `pcm` at 48 kHz/16 or 24/2, so no decoder is needed and MA resamples [S1]. It runs the
  time-filter against MA, converts each chunk's MA timestamp into chorus's server timeline, and
  hands it to the existing chorus pipeline. chorus's endpoints, protocol and servo do not change.
  Precedent: `sendspin-bt-bridge` (MIT) does exactly this for Bluetooth speakers with aiosendspin
  [S61].
- **Clock detail:** MA and chorus-server both run with host networking on the same Debian host
  (K34, [S19]). The spec timestamps on "the server's monotonic clock" [S1], and containers share
  the kernel's `CLOCK_MONOTONIC` unless a time namespace is configured (**ASSUMED**, Docker default).
  The filter should therefore converge to offset ~0 and drift ~0 over loopback, adding tens of
  microseconds at most (**ASSUMED**; the simulator can bound it).
- **Grouping:** if MA groups two chorus zones, MA sends each bridge player the same timeline and
  chorus plays each at its timestamp, so they come out in sync without chorus knowing about the MA
  group. A mixed group (a chorus zone plus a Sonos via MA's AirPlay bridge) also works if chorus
  meets the ±1 ms floor at its endpoints. chorus's own bonded sets (K30: stereo pairs, sub,
  surrounds) stay inside one chorus zone and one Sendspin player. Sendspin has no surround
  positioning yet [S11].
- **Server role (chorus-server = a Sendspin server for Sendspin speakers):** possible, but it must
  implement all 7 roles, Noise and pairing. It then competes with MA on the same network (clients
  arbitrate; "only one MA server per network" [S12]). Not recommended.
- **Line-in (K30) into MA:** chorus could later expose endpoint line-ins as Sendspin `source@v1`
  clients, so MA can route them to any player [S1]. That is optional; chorus-native line-in
  sharing does not need it.

### 1.3 Music Assistant player providers and deployment

- Providers are in-tree Python packages under `music_assistant/providers/<domain>/`, each with
  `__init__.py` and `manifest.json` (`type`: music/player/metadata/plugin/audio_analysis, `stage`,
  `requirements`, `mdns_discovery`). "Python 3.14 is minimal required". Player-provider docs
  "Will follow soon™" [S20]. Out-of-tree providers: declined by the maintainers (2025-08-12) [S21].
- `PlayerFeature` in `music-assistant-models` 1.1.214 (verified by import): `power, volume_set,
  volume_mute, pause, set_members, multi_device_dsp, seek, next_previous, play_announcement,
  enqueue, select_sound_mode, select_source, options, gapless_playback,
  gapless_different_samplerate, play_media` [S22].
- Versions in 2026: 2.8 (2026-03-25, Sendspin Bridges, player merging, Party Mode) [S13]; 2.9
  (2026-06-10, Sendspin visualizers; search snippet) [S23]; 2.10.0 (2026-08-27) through 2.10.4
  (2026-09-18) are stable; 2.11.0b3 was released 2026-09-28. MA is Apache-2.0 [S18]. 2.7
  (2025-12-17) added user profiles, per-user speaker access and WebRTC remote access [S14], plus
  mandatory login (search snippet) [S30].
- Docker: `ghcr.io/music-assistant/server`, `-v <dir>:/data`. "You must run the docker container
  with **host network mode**", for mDNS/UPnP discovery and for players "which open random TCP/UDP
  ports". Ports: 8095 (UI/API), 8097 (streams); Sendspin 8927 [S19][S17]. HA links to MA through
  the MA integration at `<ip>:8095` [S29]. **Homelab impact:** MA would be a third host-network
  exception beside chorus-server (K34), which is a homelab PR. HA itself stays on bridge networking
  and reaches MA by address.

### 1.4 A native HA custom integration

- Entity model: `MediaPlayerEntityFeature` has BROWSE_MEDIA, CLEAR_PLAYLIST, GROUPING,
  MEDIA_ANNOUNCE, MEDIA_ENQUEUE, NEXT/PREVIOUS_TRACK, PAUSE, PLAY, PLAY_MEDIA, REPEAT_SET,
  SEARCH_MEDIA, SEEK, SELECT_SOUND_MODE, SELECT_SOURCE, SHUFFLE_SET, STOP, TURN_ON/OFF,
  VOLUME_MUTE/SET/STEP. Grouping uses `async_join_players(group_members)`, `async_unjoin_player()`
  and the `group_members` property. Announce: "When the `announce` boolean attribute is set to
  `true`, the media player should try to pause the current music, announce the media to the user
  and then resume the music." TTS arrives as `play_media` of a media-source URL [S36][S39].
- What chorus would own on this path: the URL fetch and **decode** for `play_media`/TTS. HA TTS
  URLs are typically MP3 (**ASSUMED**), which brings a codec into chorus (BRIEF §3.2 gray zone).
  It would also own browse (chorus has no library) and the announce mix. Without MA there is no
  music library or streaming at all.
- Install: `config/custom_components/<domain>/` with `manifest.json`; "The version of the
  integration is required for custom integrations" [S40][S41]. HACS is not possible because the
  repo is private [S42]. In the homelab, HA's `custom_components` sits under the HA config dir,
  backed up by borgmatic and not tracked in git [S62]. Installing is therefore an owner action; the
  homelab PR can only carry the procedure.

### 1.5 Options compared (K15's letters in brackets)

| Path | What chorus builds | Sync engine stays "the project"? | Sonos-parity UX in HA | Coupling to preview protocols | Cost |
|---|---|---|---|---|---|
| **P1 Sendspin client bridge in chorus-server** [K15a] | Rust Sendspin client: WS server + mDNS per zone, Noise KKpsk2, pairing (PSK token or static code), time-filter port (~200 lines, from the Apache-2.0 C++ reference), PCM-only player role, optional metadata/controller roles | **Yes.** Endpoints, protocol and servo stay chorus's. Only the MA->server hop uses Sendspin's filter, on loopback. | **High, for free:** HA's MA integration gives browse, search, queue, announce and join/unjoin per zone [S43]. MA adds Spotify Connect and AirPlay targets per zone [S27][S28]. | Medium: RC1 spec, aiosendspin at major 9 [S5]. The coupling is confined to one module with a pinned conformance test. | ~2-3 goals: client, pairing, conformance harness, homelab MA PR |
| **P2 Sendspin-native endpoints** [K15c] | Endpoint firmware becomes a Sendspin client (sendspin-cpp, Apache-2.0) | **No.** The spec mandates the time-filter and MA's timeline on the endpoint [S1]. chorus-server is bypassed for MA audio. | High (same MA entities) | High. Firmware follows the RC. ESPHome glue is GPL [S34]. | Lower code, but it gives up the project's core |
| **P3 MA player provider "chorus"** [K15a alt / b] | A Python provider in an MA fork, streaming to chorus-server (like MA's Snapcast provider, **ASSUMED** shape) | Yes | High (same MA entities) | Low protocol risk, **high fork risk**: out-of-tree is refused [S21], MA ships weekly (2.10.1-2.10.4 in 3 weeks) [S18] | Fork rebuild every release, or an upstream PR (the Needs list, K42) |
| **P4 native HA custom integration** [K15b] | Python `custom_components/chorus`: media_player per zone with GROUPING/VOLUME/SELECT_SOURCE/ANNOUNCE | Yes | Medium: no browse or queue unless MA exists. The announce/TTS decode moves into chorus. | None | ~1-2 goals + a manual install per release (no HACS) |
| **P5 MQTT only** (K46) | Discovery payloads | Yes | Low: no media_player possible [S43] | None | Small |

**Recommendation (K15): P1, a Sendspin client bridge in chorus-server, with Music Assistant as the
source and control hub.** HA gets its media_players from HA's own MA integration. K46's MQTT
discovery adds only chorus-specific entities (section 4). Do not build P4 now. Keep it as the
fallback if MA is rejected, or for things MA cannot express. Reject P2 because it hands the sync
engine to Sendspin. Reject P3 because out-of-tree loading is refused and the fork would need a
rebuild on MA's weekly release cadence.

Simulator work goal 1 should do (K15 asks for it):
1. Model MA's Sendspin server clock -> bridge time-filter -> chorus timeline -> endpoint servo, and
   report the total error distribution against the spec's ±1 ms floor.
2. Run the same model against an in-process chorus source, so the bridge's added error is
   isolated.
3. Test a mixed MA group (a chorus zone plus a simulated Sendspin player on a separate timeline).

Conformance: run aiosendspin's server at MA's pinned version (`uv run --with
'aiosendspin[server]==9.1.1'`) as the gate's test peer. Optionally also run OHF's `conformance`
repo as a black box; it has no licence, so run it but do not copy it [S5].

What would change this: if Sendspin 1.0 changes the core again, or MA drops external players, P1
reverts to P4. If the owner rejects running MA, P4 plus MQTT becomes the whole HA story.

---

## 2. K14: music sources

| Option | Where the DRM/streaming code lives | How PCM reaches chorus | Licence / ToS | Sync impact | Verdict |
|---|---|---|---|---|---|
| **MA as the hub** | In MA (Apache-2.0, a separate container) [S18] | P1: Sendspin PCM per zone. P3: MA's HTTP/ffmpeg stream. P4: a URL chorus must fetch and decode. | Per service, carried by MA | None on P1 (timestamped) | **Recommended** |
| MA's Spotify Connect plugin | In MA, using Spotify **Soloist** (official, Linux, API key per Premium account, "Do not redistribute", builds expire after 90 days) [S56][S57] or **go-librespot** (GPL-3.0 [S54], reverse-engineered, "intended for Spotify Premium accounts created before December 2024") [S27] | Every MA player, including bridged chorus zones, "appears as its own device in the Spotify app"; groups too [S27] | MA's own note: "Spotify's terms do not clearly allow using Soloist this way ... at your own risk" [S27] | None for chorus | Take it, with the risk in MA |
| MA's AirPlay Receiver plugin | In MA ("early stage", ~5 s latency on state changes) [S28] | Exposes any MA player or group as an AirPlay target [S28] | In MA | None for chorus | Take it as a convenience |
| Per-zone AirPlay 2 receiver in chorus | In chorus or a sidecar | A PCM pipe per zone | shairport-sync: MIT-style header, but "Please refer to the individual source files for licenses" [S53]; K33 still treats it as a GPL-class reference. Permissive AirPlay 2 receivers are immature: `openairplay2` 0.5.0 MIT, 0 stars; `airguitar` Apache-2.0, "highly experimental", last push 2023 [S52]. Mature ones are GPL/LGPL (airplay2-rs, rairplay, UxPlay GPL-3.0; shairplay-rust LGPL-3.0) [S52]. FairPlay setup is reverse-engineered (**ASSUMED** legal gray). | AirPlay 2 has its own PTP timeline to map in (**ASSUMED**) | Not in chorus |
| Spotify Connect receiver in chorus (librespot, MIT [S54]) | In chorus | Direct | Spotify Developer Terms v10 (2025-05-15) prohibit "reverse-engineering ... the Spotify Platform" [S55]. The official route for hardware (eSDK) is partner-only, "companies, not individuals" (search snippet) [S58]. | Direct | Not in chorus. It breaks BRIEF §2.3, and MA already covers it. |
| Tidal Connect | Proprietary partner firmware | n/a | No public receiver SDK found. Web search surfaced only hardware partners and unofficial clients (**ASSUMED** partner-only). | n/a | Out. MA's Tidal *provider* covers playback [S26]. |
| Chromecast receiver | n/a | n/a | Receivers need Google-issued device certificates. Open implementations borrow signatures (search snippets, [S59]). | n/a | Out |

**Recommendation (K14): Music Assistant is the only music-source hub.** Its Spotify Connect and
AirPlay Receiver plugins give the "send from my phone" UX, and every such stream reaches chorus as
Sendspin PCM through the P1 bridge. chorus's codebase contains no streaming, DRM or reverse-
engineered receiver code, so BRIEF §2.3 holds unamended. chorus-native sources remain the FIFO/PCM
input, line-in (K30) and the TV path (K17).

Deployment consequence: MA becomes a homelab service (host network, `/data` volume, a Traefik
route for its UI). That is a homelab PR under K28, and account setup is the owner's (K4: the program
creates no accounts or tokens).

---

## 3. K31: voice and announcements

Facts:
- `assist_satellite` is a building-block entity. In HA 2026.9.3 it is provided by the `esphome`,
  `wyoming` and `voip` integrations (file presence verified) [S43][S38]. Its features are
  **ANNOUNCE** (`async_announce(media_id, preannounce_media_id)`, which returns only after playback
  finishes) and **START_CONVERSATION** [S37]. Actions: `assist_satellite.announce`,
  `start_conversation`, `ask_question` [S38].
- **Wyoming** (protocol lib MIT [S48]): `wyoming-satellite` is archived (MIT). Its README says
  "replaced by Linux Voice Assistant that uses the ESPHome protocol, which supports the newest
  features (e.g, media player, stop wake word, start/continue conversation, and timers)" [S47].
- **The ESPHome native API is the live satellite path for non-ESPHome devices.**
  `linux-voice-assistant` (Apache-2.0) speaks it through `aioesphomeapi` (MIT). It does wake words
  on the device with openWakeWord or microWakeWord, supports announcements, start/continue
  conversation and timers, and needs a 16 kHz mono mic [S46][S48]. Framing is protobuf over TCP
  6053, with optional Noise `NNpsk0_25519_ChaChaPoly_SHA256` (third-party doc, search snippet)
  [S64]. `api.proto` is a non-C++ file, so ESPHome's split licence makes it MIT [S34]. A Rust
  server/client crate exists: `esphome-native-api` 3.0.0, MIT, "can be used for Server and Client
  implementations" [S50].
- ESPHome `voice_assistant` needs a microphone plus a `speaker` or `media_player`. Wake word runs
  either on the device (`micro_wake_word`) or in HA (`use_wake_word`). "Crashes are likely to
  occur if you include too many additional components" [S32][S33].
- Wake word licences: openWakeWord code is Apache-2.0, but "All of the included pre-trained models
  are licensed under ... CC BY-NC-SA 4.0" [S49]. microWakeWord (Apache-2.0),
  `esphome/micro-wake-word-models` (Apache-2.0) and `pymicro-wakeword` (Apache-2.0) run on Linux
  [S48]. Piper TTS is now `piper1-gpl` (GPL-3.0); the old `rhasspy/piper` MIT repo is archived
  [S48]. That only matters if chorus ever embedded TTS, which it should not: HA does TTS.
- Echo cancellation: a speaker that listens while playing needs AEC. Espressif's ESP-SR is under
  "ESPRESSIF MIT", granted "for use on all ESPRESSIF SYSTEMS products" only [S51]. XMOS-based mic
  boards do AEC on-board (per the LVA README [S46]). K21/K36: the owner owns no mics, so this is a
  buy-list line.
- Announce on the P1 path: HA `tts.speak`/`play_media(announce)` targets MA's media_player, and MA
  runs pause -> announce -> resume for non-native players [S24]. The MA "native" duck path only
  covers ESPHome-manufactured Sendspin devices, relayed through their HA entity [S17]. Sendspin
  announcements to ESP32 groups were intermittently silent on MA 2.10.2 (open issue, 2026-09-09)
  [S25].

Options:

| Option | Build | Pros | Cons |
|---|---|---|---|
| **V1 Server-proxied ESPHome-API satellites** | Endpoints stream mic PCM to chorus-server over chorus's protocol. chorus-server presents one ESPHome-API device per room (the MIT Rust crate [S50], one port each) with `voice_assistant` + a media_player for announcements. Wake word on the server (pymicro-wakeword models, Apache-2.0) first. | Firmware stays chorus-only (K26-clean). HA sees standard `assist_satellite` + ANNOUNCE entities. chorus-server can **duck and mix** announcements over music itself, true Sonos-style. | chorus-server gains an ESPHome-API surface. HA must reach N ports on the host. The mic uplink is a new protocol stream. |
| V2 Endpoint-direct ESPHome API | Each endpoint's C firmware implements the API with its own wake word (microWakeWord on the S3, reimplemented, never reading ESPHome C++) | No server hop for voice | A large C surface on the endpoint, RAM pressure beside the audio path (ESPHome's own warning [S32]), and wake-word inference on the audio core |
| V3 Wyoming | Wyoming server per room in chorus-server | Simple protocol, MIT | Deprecated for satellites. Lacks timers, media player, continue-conversation [S47]. |
| V4 Separate voice puck per room | A stock ESPHome voice device next to each speaker | Zero chorus code | Not "chorus rooms as satellites". The two devices duck independently. |

**Recommendation (K31): V1, server-proxied ESPHome-API satellites with a chorus-owned ducking
mixer in chorus-server,** built after FLEET-10 (K32) and gated on mic hardware (a buy-list item).
The same mixer serves HA announcements to chorus's own announce entity, so announcements work
without MA's pause/resume and cover the "announcements/ducking" parity item. Wake word runs on the
server with Apache-licensed microWakeWord models. openWakeWord's bundled models are excluded
because of their NC licence. The homelab also shelved voice until "the house has devices to
control and a voice speaker" [S62].

---

## 4. K46: MQTT discovery

Verified: HA 2026.9.3 MQTT `SUPPORTED_COMPONENTS` = alarm_control_panel, binary_sensor, button,
camera, climate, cover, date, datetime, device_automation, device_tracker, event, fan, humidifier,
image, infrared, lawn_mower, light, lock, notify, number, scene, siren, select, sensor, switch,
tag, text, time, update, vacuum, valve, water_heater [S43]. The docs list the same set, so **no
`media_player`** [S44]. Device-based discovery (`<prefix>/device/<id>/config` with components
under one payload) is supported, and `origin` is "required" for device discovery [S44].

Per-zone shape over MQTT (one HA device per zone, one per endpoint):

| Entity | Platform | Notes |
|---|---|---|
| Volume | `number` (0-100, slider) | |
| Mute | `switch` | |
| Source | `select` (FIFO, line-in X, TV, MA) | |
| Group / follow zone | `select` ("none", zone names) | |
| Bass, treble, loudness, night mode, speech enhancement (K30) | `number` / `switch` | |
| Sync error, buffer fill, correction ppm, resyncs, underruns, RSSI, heap | `sensor` (diagnostic) | |
| Endpoint online | `binary_sensor` (connectivity) plus availability topics | |
| Firmware / OTA | `update` | Install limited to server-staged images |
| Identify, resync | `button` | |
| Endpoint button presses | `event` | |

Coexistence with P1: MA's HA integration owns the zone `media_player`, including its volume, mute
and grouping. The MA HA device carries `identifiers={(music_assistant, player_id)}` and **no
`connections`** (verified) [S43], and MQTT identifiers are namespaced to MQTT, so the two cannot
merge into one HA device (**ASSUMED** from device-registry matching on identifiers or connections).
To avoid duplicates, **when MA is present chorus publishes no MQTT volume/mute/source/group
entities** (or publishes them with `enabled_by_default: false`; that option's availability for MQTT
is **ASSUMED**, verify). MQTT then carries only chorus-specific entities: DSP/EQ, telemetry, OTA
`update`, buttons and events, all in the same HA area. Without MA, MQTT publishes the full table
above, which is K46's "independent of K15" floor. MQTT also rides on Mosquitto that already exists,
and it is IoT-VLAN-reachable in the target network design [S62].

**Recommendation (K46):** build the MQTT device-discovery publisher with a single config switch
(`ha_media_entities: mqtt | none`) that defaults to `none` when the Sendspin bridge is enabled.

---

## 5. Security note for K40 (HA is internet-reachable; anyone logged into HA can use enabled entities [S62])

| Path | What an HA user can do to chorus |
|---|---|
| P1 (MA media_players) | Play, pause, set volume up to 100, join/unjoin zones, play **any URL or provider item** MA can resolve, and announce arbitrary TTS in any room. MA has its own login and per-user speaker access since 2.7 [S14]; HA SSO into MA is optional (search snippet) [S30]. MA WebRTC remote listening and Party Mode guest QR access exist [S14][S13], so the owner should keep them off. The Sendspin link itself is Noise-encrypted and paired [S1], so chorus should refuse "unpaired access". |
| P2 | As P1, plus the endpoints hold MA pairing keys directly |
| P3 | As P1, plus MA-fork code paths to chorus-server |
| P4 (custom integration) | Everything the integration exposes. Its code runs **inside HA's process** with HA's privileges, and it needs direct LAN access to chorus's control API, which is unauthenticated under K40. HA compromise means that API is reachable. |
| MQTT (K46) | Change volume, mute, source, group and EQ, press buttons, and **trigger OTA** through `update`. HA admins can also `mqtt.publish` raw payloads to chorus's command topics (**ASSUMED**: action available to admins). |
| Voice (V1) | Trigger `start_conversation`/`ask_question`, which opens a mic listening window. Announce arbitrary audio. |

Mitigations to carry as requirements, whatever the path:
- A server-side and endpoint-side max-volume clamp and limiter per zone.
- An allowlist for URL fetches, if P4 ever exists.
- No mic capture offered as a shareable "source". Mic audio goes only to the Assist pipeline.
- OTA `update.install` only for images the server already holds and verified.
- Mosquitto ACLs: chorus's user limited to its topic prefix; command payloads validated and clamped.
- chorus entities stay out of Assist/MCP exposure unless listed in the homelab's `registry.json`
  allowlist (`expose_new: false` today) [S62].

---

## 6. Decision table

| Decision | Options | Recommendation | Confidence | What would change it |
|---|---|---|---|---|
| K15 HA/MA path | P1 Sendspin client bridge in chorus-server; P2 Sendspin-native endpoints; P3 MA provider (fork/upstream); P4 HA custom integration; P5 MQTT only | **P1 + HA's own MA integration for media_player; P4 held as fallback** | Medium-high | Sendspin 1.0 breaking the client core again; MA dropping or limiting external players; the owner rejecting MA as a service; simulator showing the bridge hop costs > ~0.2 ms |
| K14 music sources | MA hub (+ its Spotify Connect / AirPlay Receiver plugins); per-zone AirPlay 2 in chorus; librespot Connect in chorus; Tidal Connect; Cast receiver | **MA as the only hub; no receivers in chorus; BRIEF §2.3 unchanged** | High | Owner requiring AirPlay 2 native to each zone without MA; Spotify changing Soloist terms; MA's plugins proving unreliable |
| K31 voice + announcements | V1 server-proxied ESPHome-API satellites; V2 endpoint-direct ESPHome API; V3 Wyoming; V4 separate voice pucks | **V1 + chorus-owned ducking mixer; server-side microWakeWord; after FLEET-10; mic hardware on the buy list** | Medium | ESPHome API churn (the crate pins one protocol version [S50]); HA adding a first-class satellite protocol other than ESPHome's; the owner keeping voice shelved |
| K46 MQTT | Full zone surface over MQTT; chorus-specific entities only; none | **Device discovery with `ha_media_entities` switch: chorus-specific only when the MA bridge is on, full surface when MA is absent** | High (no MQTT media_player verified in source) | HA adding an MQTT media_player platform; HA device merge across integrations becoming possible |
| K40 note | (not a decision to change) | Clamp, allowlist, OTA and mic rules above as requirements; MA remote access and unpaired Sendspin off | Medium | HA leaving the internet, or chorus gaining its own auth |

---

## 7. Sources (all read 2026-09-29) and K33 log

| Tag | Source |
|---|---|
| S1 | https://github.com/Sendspin/spec (README.md, full text via raw.githubusercontent.com) |
| S2 | https://api.github.com/repos/Sendspin/spec/releases (tag 1.0.0-rc1, 2026-09-17) |
| S3 | https://github.com/Sendspin/spec/blob/main/LICENSE.md (Community Specification License 1.0) |
| S4 | https://github.com/orgs/Sendspin/repositories |
| S5 | GitHub API metadata/releases: Sendspin/sendspin-rs, time-filter, aiosendspin, sendspin-cpp, sync-test, conformance, sendspin-go-server (https://api.github.com/repos/Sendspin/...) |
| S6 | https://github.com/Sendspin/sendspin-cpp (README) |
| S7 | https://github.com/Sendspin/sendspin-rs (README) |
| S8 | https://github.com/Sendspin/time-filter (README) |
| S9 | https://github.com/Sendspin/sync-test (README) |
| S10 | https://github.com/Sendspin/aiosendspin (README) + PyPI aiosendspin 9.1.1 inspected with `uv run --with aiosendspin` (module list, Apache-2.0, Python >= 3.12) |
| S11 | https://www.matteralpha.com/industry-news/sendspin-1-0-airplay-chromecast-sonos |
| S12 | https://www.music-assistant.io/player-support/sendspin/ |
| S13 | https://www.music-assistant.io/blog/2026/03/25/music-assistant-2-8/ |
| S14 | https://www.music-assistant.io/blog/2025/12/17/music-assistant-2-7/ |
| S15 | https://github.com/orgs/music-assistant/discussions/4200 |
| S16 | https://github.com/orgs/music-assistant/discussions/5354 |
| S17 | https://github.com/music-assistant/server/tree/stable/music_assistant/providers/sendspin (README.md, manifest.json, provider.py, player.py, bridge_role.py, synchronizer_role.py; Apache-2.0) |
| S18 | https://api.github.com/repos/music-assistant/server (licence) and /releases |
| S19 | https://www.music-assistant.io/installation/ |
| S20 | https://github.com/music-assistant/server/blob/dev/DEVELOPMENT.md |
| S21 | https://github.com/orgs/music-assistant/discussions/4167 |
| S22 | PyPI music-assistant-models 1.1.214 inspected with `uv run --with music-assistant-models` (PlayerFeature, PlayerType, ProviderType) |
| S23 | https://www.music-assistant.io/blog/2026/06/10/music-assistant-2-9/ (web-search result snippet only, not fetched) |
| S24 | https://www.music-assistant.io/integration/announcements/ |
| S25 | https://github.com/music-assistant/support/issues/6380 |
| S26 | https://www.music-assistant.io/music-providers/ |
| S27 | https://www.music-assistant.io/plugins/spotify-connect/ |
| S28 | https://www.music-assistant.io/plugins/airplay-receiver/ |
| S29 | https://www.music-assistant.io/integration/installation/ |
| S30 | Web-search snippets for MA 2.7 authentication (https://www.music-assistant.io/first-run/, https://newreleases.io/project/github/music-assistant/server/release/2.7.0); pages not fetched |
| S31 | https://esphome.io/components/sendspin/ (and search snippet for https://esphome.io/components/media_player/sendspin/) |
| S32 | https://esphome.io/components/voice_assistant/ |
| S33 | https://esphome.io/components/micro_wake_word/ |
| S34 | https://raw.githubusercontent.com/esphome/esphome/dev/LICENSE (licence file only) + https://api.github.com/repos/esphome/esphome/releases (2026.9.1) |
| S35 | https://www.cnx-software.com/2026/04/21/diy-sendspin-audio-receiver-supports-multi-room-audio-synchronization-integrates-with-home-assistant/ |
| S36 | https://developers.home-assistant.io/docs/core/entity/media-player/ |
| S37 | https://developers.home-assistant.io/docs/core/entity/assist-satellite/ |
| S38 | https://www.home-assistant.io/integrations/assist_satellite/ |
| S39 | https://www.home-assistant.io/integrations/media_player/ |
| S40 | https://developers.home-assistant.io/docs/creating_integration_file_structure/ |
| S41 | https://developers.home-assistant.io/docs/creating_integration_manifest/ |
| S42 | https://www.hacs.xyz/docs/faq/private_repositories/ (and search snippets on token workarounds) |
| S43 | Home Assistant core tag 2026.9.3 (Apache-2.0), via raw.githubusercontent.com: `components/mqtt/const.py` (SUPPORTED_COMPONENTS), `components/mqtt/` directory listing, `components/music_assistant/{media_player.py,entity.py,manifest.json}`, presence of `{esphome,wyoming,voip}/assist_satellite.py`, absence of `components/sendspin/`, `helpers/device_registry.py` (function names only) |
| S44 | https://www.home-assistant.io/integrations/mqtt/ |
| S46 | https://github.com/OHF-Voice/linux-voice-assistant (README) |
| S47 | https://github.com/rhasspy/wyoming-satellite (README deprecation notice; API: archived, MIT) |
| S48 | GitHub API licences: OHF-Voice/wyoming, OHF-Voice/linux-voice-assistant, dscripka/openWakeWord, kahrendt/microWakeWord, OHF-Voice/pymicro-wakeword, esphome/micro-wake-word-models, rhasspy/wyoming-openwakeword, OHF-Voice/piper1-gpl, rhasspy/piper, esphome/aioesphomeapi |
| S49 | https://github.com/dscripka/openWakeWord (README licence section) |
| S50 | https://crates.io/crates/esphome-native-api (3.0.0, MIT) + https://github.com/UbiHome/esphome-native-api (README) |
| S51 | https://github.com/espressif/esp-sr (LICENSE) |
| S52 | GitHub API licences: lmcgartland/airplay2-rs, MSNexploder/airguitar, r4v3n6101/rairplay, ibiscum/shairplay-rust, FDH2/UxPlay, openairplay/airplay2-receiver, juhovh/shairplay; crates.io openairplay2 (0.5.0, MIT) and st3fan/openairplay2 metadata |
| S53 | https://github.com/mikebrady/shairport-sync LICENSES and COPYING files (licence files only; README grepped for "licen/gpl", no hits) |
| S54 | GitHub API licences: librespot-org/librespot (MIT), devgianlu/go-librespot (GPL-3.0) |
| S55 | https://developer.spotify.com/terms |
| S56 | https://developer.spotify.com/blog/2026-08-13-introducing-spotify-soloist |
| S57 | https://developer.spotify.com/documentation/soloist/tutorials/getting-started (and /features) |
| S58 | Web-search snippets for https://developer.spotify.com/documentation/commercial-hardware/ (eSDK partner-only); not fetched |
| S59 | Web-search snippets: https://github.com/orgs/music-assistant/discussions/2354, https://github.com/rgerganov/shanocast (Cast device authentication); not fetched |
| S60 | Web search "TIDAL Connect SDK": no public receiver SDK found (negative result) |
| S61 | https://github.com/trudenboy/sendspin-bt-bridge (README) |
| S62 | Local: /cache/tmp/plan-2026-09-chorus/survey-homelab.md §0, §2, §3; homelab-ro/docs/home-automation.md (HA config backup, "What can change things") |
| S63 | Local: /workspace/BRIEF.md §0-§5.8; /cache/tmp/plan-2026-09-chorus/decisions.md |
| S64 | Web-search snippet: https://www.mintlify.com/richard87/esphome-apiclient/advanced/protocol (port 6053, Noise NNpsk0); not fetched |

Also fetched and yielding nothing usable: https://www.hacs.xyz/docs/faq/custom_repositories/,
https://www.whathifi.com/features/tidal-connect-everything-you-need-to-know (truncated), and
https://www.music-assistant.io/faq/announcements/ (404).

**K33 log.** No GPL source was opened. Not read: any source of Snapcast, snapcast-rs,
shairport-sync, squeezelite, go-librespot, UxPlay, airplay2-rs, rairplay, shairplay-rust (LGPL),
piper1-gpl, or ESPHome's C++ components (`sendspin`, `voice_assistant`, `micro_wake_word`: GPLv3
under ESPHome's split licence). What was read:
- Only the licence files of shairport-sync and ESPHome.
- The first 5 lines of ESPHome `api.proto`, a `.proto` file, which is MIT under ESPHome's
  licence split.
- Permissive source: MA's Sendspin provider (Apache-2.0) and HA core 2026.9.3 files (Apache-2.0),
  as listed in S17 and S43.
- Installed packages were imported to list their API only, without reading their source:
  aiosendspin 9.1.1 (Apache-2.0) and music-assistant-models 1.1.214 (Apache-2.0).
