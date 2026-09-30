# Research: the chorus Home Assistant integration (K61, K59, K54, K77, K78, K71, K73, K83, K84, K93, K46, K40)

Date: 2026-09-29. Target: Home Assistant 2026.9.3 (homelab pin; a container, not HA OS; HACS present
but unable to install a private repo). Status: COMPLETE for the time budget; ASSUMED items are marked.

Citation convention: `[Hn]` / `[Ln]` tags resolve in section 9, where every source carries its URL
and "read 2026-09-29". Clean-room: only Apache-2.0 files (HA core tag 2026.9.3, HA frontend `dev`)
and ESPHome's MIT `api.proto` were opened; no GPL source was opened (K33 log in section 9).

---

## 0. Headline

1. **Shape (K61):** a config-flow, `local_push`, zeroconf-discovered custom integration `chorus`
   that self-certifies against **all Platinum-tier rules** with a `quality_scale.yaml`, because
   hassfest skips that file for custom integrations [H3]. A CI job copies the integration into an
   HA core 2026.9.3 checkout and runs hassfest there, so the check is real, not claimed. Tests use
   `pytest-homeassistant-custom-component` 0.13.366 (MIT, pinned to HA 2026.9.3) [H5] against the
   same JSON fixtures the Rust server tests use (CLAUDE.md rule 5). Transport: keep chorus's
   existing HTTP + SSE control API [L3], add one binary streaming route for voice. **No runtime
   `requirements`**: the client library is vendored inside the integration, so HA never runs pip.
   Install: a **pinned, vendored copy in the homelab repo, bind-mounted read-only** (the pattern the
   homelab already uses for custom cards [L2]), refreshed by a chorus-opened homelab PR per release.
2. **Entities (K59/K54/K77/K78):** one `media_player` per room (GROUPING, leader-first
   `group_members`, like Sonos [H9]); one `media_player` per saved group (always present; its
   volume is the Sonos-style group volume, like MA's group players [H10]; **no** GROUPING flag so
   it does not pollute HA's join dialog [H13]); live groups appear as room `group_members`, plus a
   per-room "Group volume" `number`. No per-live-group entities (entity churn).
3. **Voice (K71): (a), chorus's own `assist_satellite` per room.** Server-side wake word (K73) maps
   exactly onto what ESPHome does for on-device wake words: start the pipeline at STT with
   `wake_word_phrase` [H16][H17]. Timers, announce (with pre-announce), start_conversation and
   ask_question are all reachable through public core APIs that three core integrations use
   [H16][H18][H19]. (b) ESPHome-API emulation is declined: a second device per room, N ports,
   a pre-1.0-quality Rust crate that majored three times in 15 months [H23], and an api.proto that
   is still reshaping its capability messages [H22]. This supersedes the earlier V1 recommendation
   [L4 §3], which predates K61.
4. **Dashboard (K84): stock cards first, then a small set of custom tile features**, not a full
   custom card. HA 2026.9 already has tile features for playback, volume slider + mute, volume
   buttons, filtered source and sound mode [H14], and a join/unjoin dialog [H13][H14]. Missing:
   Sonos-style group volume on live groups and a whole-house overview; the PWA (K16) owns
   drag-to-group.
5. **MQTT (K46): limit to "deferred, off by default, never alongside the integration".** Every
   entity K46 would publish is now an integration entity; HA would show duplicates.
6. **Security (K40):** eleven requirements in section 7, chief among them: `play_media` fetches
   only HA-origin URLs, server-side volume clamps win over HA, firmware install only for
   server-held verified images and behind an integration option, and mic audio only while a
   pipeline runs.

---

## 1. K61: integration shape

### 1.1 The Integration Quality Scale in 2026 (verified in source)

The rule list and tiers below are taken from hassfest's own table in HA core 2026.9.3 [H3] and match
the developer docs [H2]. Tiers are cumulative.

| Tier | Rules |
|---|---|
| Bronze (20) | action-setup, appropriate-polling, brands, common-modules, config-flow, config-flow-test-coverage, dependency-transparency, docs-actions, docs-conditions, docs-high-level-description, docs-installation-instructions, docs-removal-instructions, docs-triggers, entity-event-setup, entity-unique-id, has-entity-name, runtime-data, test-before-configure, test-before-setup, unique-config-entry |
| Silver (10) | action-exceptions, config-entry-unloading, docs-configuration-parameters, docs-installation-parameters, entity-unavailable, integration-owner, log-when-unavailable, parallel-updates, reauthentication-flow, test-coverage (above 95% [H6]) |
| Gold (21) | devices, diagnostics, discovery, discovery-update-info, docs-data-update, docs-examples, docs-known-limitations, docs-supported-devices, docs-supported-functions, docs-troubleshooting, docs-use-cases, dynamic-devices, entity-category, entity-device-class, entity-disabled-by-default, entity-translations, exception-translations, icon-translations, reconfiguration-flow, repair-issues, stale-devices |
| Platinum (3) | async-dependency, inject-websession, strict-typing |

Facts that shape "core quality" for a custom integration:
- HA "does not review, security audit, maintain, or support third-party custom integrations";
  a custom integration cannot hold a tier, only align with one [H1].
- hassfest can validate one integration by path (`--integration-path`) and then runs the
  per-integration plugins (manifest, translations, icons, services, config_flow, quality_scale...)
  [H4]. **But `validate_iqs_file` returns immediately when the integration is not under
  `homeassistant/components`** [H3], and "core" is decided purely by path [H4]. A custom
  integration's `quality_scale.yaml` is therefore never checked unless it is placed inside a core
  checkout.
- Custom integrations must carry `version` in `manifest.json` (AwesomeVersion SemVer/CalVer) [H8][H4].
- Since HA 2026.3 a custom integration can ship `brand/icon.png`, `logo.png` (and dark / @2x
  variants) in its own directory [H7][H29]; the brands repo is closed to private projects anyway.
- `dependency-transparency` requires the device library to be OSI-licensed, on PyPI, built by a
  public CI and tagged in an open repo, "no exceptions" [H6]; core review requires all API code in
  a PyPI library [H6]. A private repo cannot meet this.
- `inject-websession` means the library takes HA's shared aiohttp session; `strict-typing` means
  mypy strict plus a PEP 561 `py.typed` library [H6].
- Custom integrations "should only include requirements that are not required by the Core
  requirements.txt" [H8]. In a Docker (non-venv) HA, `pip_kwargs` sets no `target`, so runtime
  `requirements` are installed (with uv) into the container's own site-packages at start-up [H30]:
  fetched from PyPI at runtime and lost on every container recreate, bypassing the homelab's
  digest-pinned image.

**Recommendation R1.1 (core-quality, concretely):** target **Platinum, self-certified and
machine-checked**:
- `custom_components/chorus/quality_scale.yaml` lists every rule as `done` or `exempt` with a
  comment. Expected exemptions: `reauthentication-flow` (chorus has no credentials, K40),
  `dependency-transparency` (private; the client library is vendored, see below; flips to `done`
  when K42's upstream step publishes it), `docs-*` are satisfied by `integrations/ha/README.md`
  plus a user doc in chorus's docs (no home-assistant.io page).
- CI gate `ha-hassfest`: clone HA core at the pinned tag, copy the integration to
  `homeassistant/components/chorus` (drop `version`), run
  `python -m script.hassfest --integration-path homeassistant/components/chorus`. This runs the
  real Platinum validators (config_flow, runtime_data, test_before_setup, unique_config_entry,
  discovery, reconfiguration_flow, strict_typing) [H3]. mypy `--strict` runs separately.
- Client library: an async, fully typed `aiochorus` package (own tests, `py.typed`, takes an
  injected `aiohttp.ClientSession`) that lives in the chorus repo and is **vendored into the
  integration at release build time** (`custom_components/chorus/_aiochorus/`), so `manifest.json`
  has `"requirements": []` and HA never pip-installs anything. aiohttp is already in HA core.
  Upstream (K42) later publishes `aiochorus` to PyPI and swaps the vendored copy for a pinned
  requirement: one line in the manifest.

**Testable outside HA core (R1.2):** `pytest-homeassistant-custom-component` is MIT, requires
Python >= 3.14, and is regenerated from each HA release: **0.13.366 = HA 2026.9.3** (uploaded
2026-09-19), 0.13.367 = HA 2026.9.4 (2026-09-27) [H5]. It brings HA's own `hass` fixture,
`MockConfigEntry`, and the syrupy snapshot extension [H5]. Pin it to the version whose HA matches
the homelab pin; a bump of the homelab HA pin is a chorus task (re-run the suite at the new
version). Everything in the rule table is testable this way except the docs rules (reviewed by
hand) and `brands` (file presence).

### 1.2 How the integration talks to chorus-server

Today's control plane [L3]: HTTP on `--control-listen`, `GET /api/state` (the state message),
`GET /api/events` (`text/event-stream`, one full state message per change, starting with the
current state), `POST /api/command` (one control message; 200/400/426), `Connection: close`
on every request, each subscriber bounded at 32 queued messages and dropped past that. No auth by
design. mDNS: `_chorus-audio._tcp.local.` and `_chorus-ctl._tcp.local.` with TXT `v=<catalog
version>` [L3].

| Option | For | Against |
|---|---|---|
| **T1 HTTP + SSE (existing)** | Already the contract the PWA and verification scripts use ("the UI and the verification scripts are the same subscriber" [L3]); full-state messages make reconnect trivially correct (a dropped HA subscriber reconnects and gets the whole state); aiohttp reads SSE natively | Text only; a command is a new TCP connection (sub-ms on a LAN, **ASSUMED** fine for HA) |
| T2 new WebSocket | Bidirectional, binary frames (voice audio) | A second subscriber protocol, which the control-plane doc deliberately avoided [L3]; WebSocket was already replaced by HTTP + SSE once, with no ADR [L5] |
| T3 ESPHome native API | HA already speaks it | Wrong shape for rooms and groups (see 3.3) |

**Recommendation R1.3: T1**, `iot_class: local_push`, plus exactly one new route for voice: a
chunked binary `GET` that streams 16 kHz mono s16le PCM for one pipeline run (section 3).
Two constraints the integration adds to the server:
- **Throttle what HA sees.** Every HA state change is written by the recorder. The SSE state
  message should carry telemetry (sync error, buffer fill...) at a bounded rate (for example once
  per 30 s or on threshold crossing), leaving high-rate series to Prometheus/Grafana (K46).
  The integration additionally drops unchanged values.
- **Versioned catalog.** The TXT `v=` and the `hello` command gate compatibility; the integration
  raises a repair issue (`repair-issues`) when the server's catalog is newer than it understands.

### 1.3 Discovery (zeroconf)

- A manifest `"zeroconf": ["_chorus-ctl._tcp.local."]` makes HA's zeroconf integration call the
  config flow's `zeroconf` step; very generic types need a name/properties filter, chorus's does
  not [H8]. Custom integrations may declare zeroconf matchers (the manifest docs apply to both)
  [H8].
- Unique ID: a stable server ID. **Add `id=<server uuid>` to the `_chorus-ctl` TXT record** (today
  it carries only `v=`) [L3], so `discovery-update-info` can update the host/port on an existing
  entry instead of creating a duplicate.
- The homelab runs HA on bridge networks; an mDNS reflector repeats 5353 between the LAN and the
  `homeassistant` bridge only, and the house rule is "Discovery is a convenience; every
  integration points at a reserved address" [L2]. So the `user` step (host + port) is primary and
  discovery is the convenience; `reconfiguration-flow` edits the host.

### 1.4 Installation without HACS

HACS cannot install private repos [L4 S42]. The homelab keeps HA's `custom_components` on the host,
untracked and only backed up by borgmatic [L2], but it **already vendors third-party browser code
into git with a lock file** (`www/cards/` + `cards.lock`, bound read-only at `/config/www/cards`)
[L2].

| Option | How | For | Against |
|---|---|---|---|
| I1 Manual copy | Owner copies a release tarball into `/opt/docker/config/homeassistant/custom_components/chorus` | Zero homelab change | Untracked, drifts, needs sudo each release |
| **I2 Vendored + read-only bind** | Homelab carries `home-automation/homeassistant/custom_components/chorus/` plus a `components.lock` line (release, sha256); compose adds a read-only bind to `/config/custom_components/chorus`; HA `check_config` in homelab CI then loads it | Code-as-config like the cards; reviewable diffs; homelab CI validates it; restore = git | A homelab PR per release (K28: chorus opens, owner merges); CPython cannot write `__pycache__` into a read-only dir and silently skips it (**ASSUMED**, standard CPython behaviour, not tested here) |
| I3 Owner-run install script | `scripts/chorus-ha-install.sh <version>`: fetch the release asset with `gh release download` (private repo needs the owner's token), verify sha256 from a lock, unpack, restart HA | Works without a homelab compose change | Needs a GitHub token on the host; still untracked in git |
| I4 HACS with a token | Not supported for private repos [L4 S42] | | |

**Recommendation R1.4: I2**, with I3's verify step reused as the homelab helper that produces the
lock line and the vendored tree (the same shape as `scripts/ha-cards-update.sh` [L2]). The release
artefact is a tarball of `custom_components/chorus` (with the vendored `_aiochorus` and `brand/`),
its sha256 in chorus's release notes. The the Needs list item per release shrinks to "merge the homelab
PR, restart HA".

---

## 2. media_player per room and per group (K59, K54, K77, K78)

### 2.1 Prior art (read in source)

- **Sonos (core, quality `bronze`, `local_push`, zeroconf `_sonos._tcp.local.`)** [H9]: one
  entity per speaker (`_attr_name = None`, the device name is the entity name);
  `group_members` = `speaker.sonos_group_entities`, rebuilt on every regroup and shared by every
  member, coordinator first; all transport and `play_media` go to `self.coordinator` (group
  leader); volume is per speaker. `async_join_players(group_members)` resolves entity IDs through
  the entity registry and raises translated errors for unknown ones; `async_unjoin_player`
  coalesces unjoins over a short window because "Removing coordinators last better preserves
  playqueues on the speakers". **No group entity and no group volume in HA.** Announce: `play_media(announce=True)`
  plays a clip over the websocket API, supports MP3 and WAV, takes `extra.volume`, and falls back
  to normal playback when the speaker refuses clips.
- **Music Assistant (core)** [H10]: group players are permanent `media_player` entities; for
  `PlayerType.GROUP` the entity's `volume_level` is **`player.group_volume`**; `group_members` is
  translated from MA player IDs to entity IDs via the entity registry; GROUPING is set only when
  the player supports setting members.
- **HA core semantics** [H11][H12]: `group_members` "If the platform has a concept of defining a
  group leader, the leader should be the first element"; `media_player.join` requires the
  GROUPING feature; **the `group_members` attribute is only published when GROUPING is supported**.
  `volume_step` (default 0.1) drives `volume_up`/`down`.
- **HA frontend join dialog** (since 2025.6, "join or unjoin groups of media players" from the
  media player card [H14]) [H13]: lists every `media_player` of the **same platform** that
  supports GROUPING, pre-ticks current `group_members`, and on submit calls `join` on the opened
  entity with the ticked set, then `unjoin` on each unticked former member. No group volume.

### 2.2 Proposed model

| Chorus concept | HA representation | Features | Semantics |
|---|---|---|---|
| Room (bonded set inside, K54) | `media_player.<room>`, one per room | GROUPING, VOLUME_SET/STEP/MUTE, SELECT_SOURCE, SELECT_SOUND_MODE (optional), PLAY/PAUSE/STOP (when the input supports it), PLAY_MEDIA, MEDIA_ANNOUNCE, BROWSE_MEDIA, TURN_ON/OFF | Volume = this room. Transport and `play_media` act on the room's group (Sonos). `group_members` = live group, leader first, `[self]` when solo |
| Saved group ("Downstairs", "Whole house") | `media_player.<group>`, one per saved group, always present | VOLUME_SET/STEP/MUTE, PLAY_MEDIA, MEDIA_ANNOUNCE, BROWSE_MEDIA, SELECT_SOURCE, TURN_ON/OFF; **not** GROUPING | `play_media`/`select_source`/`turn_on` assemble the group with K78 "take the room" and play; state `playing` when the group is assembled, `off`/`idle` otherwise; volume = Sonos-style group volume (K77). Members shown as a `rooms` extra attribute (entity IDs), because `group_members` needs GROUPING [H11] |
| Live ad-hoc group | Room entities' `group_members`; no entity of its own | | Formed by `media_player.join` (from the frontend dialog or automations), the PWA, chorusctl or K78 |
| Group volume of a live group (K77) | `number.<room>_group_volume` per room (0-100, slider), `available` only while the room is grouped | | Setting it scales every member relatively (server-side), same as the PWA's group slider |

`join`/`unjoin` mapping:
- `media_player.join(entity_id=kitchen, group_members=[den, patio])`: kitchen is the leader; den
  and patio each **leave whatever they were playing or following** and follow kitchen's stream
  (K78 take-the-room). Members already in kitchen's group are a no-op. A group entity in
  `group_members` is refused with a translated `ServiceValidationError` (`action-exceptions`).
- `media_player.unjoin(den)`: den leaves and goes idle. If the leader unjoins, the next member
  becomes leader and keeps the stream (Sonos). Coalesce unjoins like Sonos so the frontend's
  "unjoin N members" burst becomes one server command.

Why saved groups do **not** get GROUPING: the join dialog lists every same-platform GROUPING
player [H13], so group entities would appear as tick-boxes inside every room's dialog and the
only honest answer to ticking one is an error. Membership of saved groups is edited in the PWA or
chorusctl (K54: chorus owns grouping). Cost: HA cannot edit saved-group membership, and the
`rooms` attribute is an extra state attribute (core reviewers tolerate but discourage them,
**ASSUMED**). Alternative G2 (MA-style GROUPING on group entities [H10]) is listed in section 8.

Why no per-live-group entities: they would be created and deleted as people group rooms, leaving
registry churn, orphaned entity IDs in automations, and `stale-devices` work for no gain over
`group_members`.

### 2.3 Devices and naming

- HA device registry: `has_entity_name = True` on every entity (Bronze) and `_attr_name = None` on
  the primary entity so `media_player.kitchen` takes the device name [H9].
- **Recommended device tree:** one **server** device (the config entry's hub; holds saved-group
  media_players, server diagnostics, server-level update if any); one **room** device per room
  (`suggested_area` = room name; holds the room media_player, sound controls, group-volume number,
  assist_satellite); one **speaker** device per endpoint, `via_device` = its room device, with
  `connections={(mac, <mac>)}` and `sw_version`/`hw_version` (holds diagnostics, button events,
  firmware update). A saved group is its own small device under the server (so the entity takes the
  group's name). `dynamic-devices` (new speakers adopted, K-autoadopt) and `stale-devices`
  (speaker retired, room deleted) are both Gold rules and both apply.
- Caution: a MAC in `connections` can merge devices across integrations (UniFi also knows the
  speaker's MAC): the registry's `get_entry` matches on identifiers or connections and, with no
  config entry given, returns the first match from any config entry [H30]. That merge is
  desirable (one device, two integrations); confirm on the homelab once a speaker exists.

### 2.4 play_media, announce, browse (K64 inputs-only)

- **Inputs, not content.** `browse_media` root = a "Chorus inputs" directory (line-ins, TV path,
  UPnP renderers, Soloist sidecars: each a playable `chorus://input/<id>` item, which is also what
  `select_source` names), plus HA's own media sources passed through with an audio content filter
  (`media_source.async_browse_media(hass, id, content_filter=...)` is the core helper [H28];
  the pattern is the dev docs' "no own media sources" example [H12]). HA's media sources are HA's
  content (TTS, local media, radio browser if enabled), so K64 stays true of chorus itself.
- **play_media inputs accepted:** (1) `media-source://` IDs, resolved in HA with
  `media_source.async_resolve_media` then `async_process_play_media_url` [H9][H12], which yields
  an absolute, signed URL to HA itself; (2) `chorus://input/<id>`; (3) anything else is refused
  unless its origin equals HA's own internal/external URL (security R7.2).
- **Formats chorus must decode.** TTS output defaults to **MP3** (`_DEFAULT_FORMAT = "mp3"`), with
  per-request `preferred_format`, `preferred_sample_rate`, `preferred_sample_channels`,
  `preferred_sample_bytes`, `preferred_bitrate` options converted by ffmpeg in HA [H15]. The
  assist_satellite announce path applies the satellite's own `tts_options` [H16], so satellite
  announcements can be requested as FLAC or WAV at 48 kHz. But the default pre-announce chime is
  `preannounce.mp3` [H16], `tts.speak` to a media_player uses the caller's options, and local
  media is whatever the user has. **chorus-server needs an MP3 decoder regardless** (plus
  WAV/FLAC; AAC/OGG if radio streams are allowed). This is the BRIEF §3.2 codec gray zone; a pure
  Rust decoder crate is the likely choice (licence to be checked when chosen; not researched here).
- **announce:** `play_media(announce=True)` means "pause the current music, announce, resume"
  [H12]. chorus does better with its ducking mixer (K31): duck the room (or its whole group),
  mix the clip, restore. `extra.volume` (Sonos precedent [H9]) sets the announcement level, clamped
  by the room limit. The same mixer serves the satellite's `async_announce`.

---

## 3. K71: the voice path

### 3.1 Facts about option (a): chorus's own `assist_satellite` entity per room

- `AssistSatelliteEntity` is the building block introduced in HA 2024.10; `esphome` and `voip`
  moved to it first, `wyoming` next [H21]. In 2026.9.3 all three use it [H17][H18].
- Features: `ANNOUNCE = 1`, `START_CONVERSATION = 2` [H16]. Actions: `announce`,
  `start_conversation`, `ask_question` (response-only) [H16].
- **Feeding audio:** the integration calls
  `async_accept_pipeline_from_satellite(audio_stream, start_stage=STT, end_stage=TTS,
  wake_word_phrase=None)`; the pipeline's STT metadata is fixed at **WAV/PCM, 16-bit, 16000 Hz,
  mono** [H16]. The same call carries the satellite's `tts_options` to the TTS stage [H16].
- **Server-side wake word (K73) fits:** ESPHome starts at `PipelineStage.WAKE_WORD` only when the
  device asks HA to run wake word; when the device already detected it, it starts at
  `PipelineStage.STT` and passes `wake_word_phrase` [H17]. chorus-server detecting the wake word
  is, from HA's view, exactly that. The wake-word intercept path used by HA's satellite setup
  wizard only accepts start at STT with a phrase ("Only on-device wake words currently
  supported") [H16], which again is what chorus would do.
- **Wake-word selection UI:** `async_get_configuration` / `async_set_configuration` return and
  set available and active wake words (`AssistSatelliteConfiguration`) [H16]; chorus can offer its
  server-side microWakeWord models there (K73's Apache-2.0 models).
- **Announce:** `async_announce(announcement)` receives `media_id` (already an absolute HA URL),
  `preannounce_media_id` (default `/api/assist_satellite/static/preannounce.mp3`), `message`,
  `media_id_source` (`tts`/`media_id`/`url`); it must return only after playback [H16][L4 §3].
- **Timers:** `intent.async_register_timer_handler(hass, device_id, handler)`; events are
  started/updated/cancelled/finished with name, seconds left [H19]. VoIP and Wyoming both
  register this way [H18].
- **Continue conversation:** the pipeline sets `continue_conversation` in the intent-end data;
  ESPHome forwards it to the device, which restarts listening [H20][H17]. A chorus satellite
  watches `on_pipeline_event` and tells the server to reopen the room's mic and start a new run at
  STT.
- **Support matrix in core 2026.9.3:** ESPHome ANNOUNCE + START_CONVERSATION + timers per device
  feature flags [H17]; VoIP ANNOUNCE + START_CONVERSATION + timers; Wyoming ANNOUNCE + timers, no
  START_CONVERSATION [H18]. A custom integration can implement the full set.

### 3.2 Facts about option (b): chorus-server emulating ESPHome-API devices

- HA's ESPHome integration is `local_push`, platinum, connects as a **client** to each device's
  host:port (discovered via `_esphomelib._tcp.local.`) and depends on `assist_pipeline`, `intent`,
  `ffmpeg` [H17 manifest]. One ESPHome device = one config entry = one listening port on
  chorus-server = one extra HA device per room, beside the chorus room device (two devices per
  room; the ESPHome one would also carry its own media_player for announcements, duplicating the
  room entity).
- api.proto (MIT under ESPHome's split licence [L4 S34]): voice uses `VoiceAssistantRequest`
  (`start`, `conversation_id`, `flags` incl. USE_WAKE_WORD, `wake_word_phrase`),
  `VoiceAssistantAudio` (`data`, `end`, `data2`), `VoiceAssistantTimerEventResponse`,
  `VoiceAssistantAnnounceRequest` (`media_id`, `text`, `preannounce_media_id`,
  `start_conversation`), `VoiceAssistantConfigurationRequest/Response` [H22]. Feature flags
  (`API_AUDIO`, `ANNOUNCE`, `START_CONVERSATION`, `TIMERS`, `SPEAKER`, `MULTI_CHANNEL_AUDIO`)
  decide what HA enables [H17]. The `dev` proto is **still moving**: capabilities are migrating
  to a new `DeviceCapabilitiesRequest/Response` for API >= 1.15, with `DeviceInfoResponse` kept for
  older clients [H22].
- `esphome-native-api` (MIT): 3.0.0 on 2026-07-15; majors 1.0 (2025-04), 2.0 (2025-12), 3.0
  (2026-07); "This is still work in progress, so the API surface may change"; "This crate only
  supports one ESPHome protocol version"; "Support for multiple ESPHome versions via feature flags
  (not yet implemented)"; 14 stars; deps include `prost` and `noise-protocol` [H23].

### 3.3 Comparison

| Criterion | (a) own assist_satellite | (b) ESPHome-API emulation |
|---|---|---|
| API that can break chorus | HA's Python entity API (`AssistSatelliteEntity`, `intent` timers): internal, but used by 3 core integrations; the integration is pinned and tested per HA release (R1.2) | ESPHome wire protocol as HA's ESPHome client interprets it; api.proto churn [H22]; one-protocol-version crate that majored 3 times in 15 months [H23] |
| Features | Everything the base class offers: announce + pre-announce, start_conversation, ask_question, timers, continue conversation, wake-word selection | Same set, gated by feature flags [H17] |
| HA devices per room | One (the room device carries media_player + satellite) | Two, plus a duplicate media_player for announcements |
| Server surface | One streaming route on the existing control plane | N extra TCP listeners, Noise PSKs, protobuf framing, per-room fake MACs |
| Ducking / group awareness | Direct: the integration knows rooms and groups | Indirect: ESPHome's media_player path knows nothing of chorus groups |
| Effort | Python in the integration (~one module) + one server route | A Rust ESPHome server in chorus-server, device emulation, plus the integration anyway (K61) |
| Works without the chorus integration | No | Yes (HA's own ESPHome integration) |

**Recommendation R3 (K71): (a).** Confidence medium-high. The one real advantage of (b), working
without chorus's integration, no longer matters once K61 makes the integration the product's HA
face. Carry into the goal: mic streams only while a pipeline runs (K73 wake word runs on the
server, which receives mic audio continuously while voice is enabled and the hardware mute is off;
HA receives audio only after a wake word); AEC and hardware remain gated on the mic buy-list item
(K21/K36) as before [L4 §3].

---

## 4. K84: dashboard UI

What stock HA 2026.9 does for media players (release notes and docs [H14], frontend source [H13]):
- Tile card features: **playback** (pickable, reorderable controls; since 2026.6 also shuffle,
  repeat, volume up/down, mute), **volume slider** (2025.1; mute button since 2026.6), **volume
  buttons**, **source** and **sound mode** dropdowns (2026.5; filterable lists since 2026.6).
- Media control card; redesigned more-info dialog with artwork (2025.10); a **join/unjoin dialog**
  from the media player UI (2025.6): tick same-platform GROUPING players [H13].
- Not present: any group-volume control, drag-to-group, or a rooms overview that shows groups.

| Option | What | For | Against |
|---|---|---|---|
| **U1 Stock only** | A "Music" section: one tile per room (playback, volume slider + mute, source filtered to that room's inputs), one tile per saved group (group volume), the per-room group-volume `number` shown as a tile; join via the stock dialog | Zero browser code; survives HA frontend upgrades; matches the homelab rule "native Sections, tile, heading and graph cards come first" [L2] | Live-group volume is a separate tile; no at-a-glance group map |
| **U2 Custom tile features** | 1-2 small features (`custom:chorus-group-volume`, `custom:chorus-group-chips`) that plug into stock tile cards [H27] | Small; composes with stock tiles; card editor picks them up | Browser code to pin and test; frontend API is informal |
| U3 Full custom card | `chorus-rooms-card`: rooms, drag-to-group, group volume, now playing, inputs | Closest to the PWA in HA | Largest; duplicates the PWA (K16); the homelab treats cards as code that "runs with the user's HA session" and allows one only where native falls short [L2] |
| U4 Embed the PWA | iframe panel | No new UI | The PWA sits behind Traefik `lan-only` + login (K40); HA is internet-reachable, so it breaks off-LAN |

**Recommendation R4: U1 in the first HA goal; U2 as a later, optional goal**; U3 declined unless
the owner wants drag-to-group inside HA (the PWA owns it). Delivery for U2: the integration serves
its JS with `hass.http.async_register_static_paths([StaticPathConfig(...)])` (the same call
`assist_satellite` uses for its chime [H16]) and the **homelab registers it explicitly** in YAML
`lovelace: resources` (the homelab runs `resource_mode: yaml` [L2]) with a `?v=<version>` cache
buster. `frontend.add_extra_js_url` exists and is documented for custom integrations [H26], but it
loads the module on every page for every user without a line in the homelab repo, which bypasses
the homelab's explicit-card rule; do not use it. Serving from the integration keeps card and
integration versions locked together. Card suggestions (`getEntitySuggestion`, HA 2026.6) are a
free nicety for U2/U3 [H27].

---

## 5. K83/K93: the other entities

| K83/K93 item | Platform | Category / flags | Notes and HA constraints |
|---|---|---|---|
| Bass, treble | `number` (slider, dB) | `config` | Room device |
| Loudness, night mode, speech enhancement | `switch` | `config` | Room device; `entity-translations` names |
| Input select | **the room media_player's `source`** | | Do not add a duplicate `select`; the tile source feature covers the UI [H14] |
| Autoplay (TV, line-in), quiet hours on/off | `switch` | `config` | Quiet-hours window as two `time` entities + cap `number`, or left to the PWA (**proposed**: PWA, to keep HA lean) |
| Per-room volume limit (K81) | `number` | `config` | The server enforces it; the HA entity only displays and sets it (R7.1) |
| Group volume (K77) | `number` per room | none | Section 2.2 |
| Sync error, buffer fill, corrections, resyncs | `sensor` | `diagnostic`, **disabled by default** except sync error | `entity-disabled-by-default` (Gold); rate-limited (1.2); `state_class: measurement` |
| Link, RSSI, temperature | `binary_sensor` (connectivity), `sensor` (`signal_strength`, `temperature` device classes) | `diagnostic` | `entity-device-class` (Gold) |
| Firmware version | **device registry `sw_version`** | | Not a sensor; the `update` entity shows it too |
| Speaker button presses | `event`, `EventDeviceClass.BUTTON`, one entity per physical button | none | Stateless; `event_types` must be declared up front, firing an undeclared type raises `ValueError`; the standard button types are `press_start`, `press_end`, `long_press_start`, `long_press_end`; separate entities per direction preferred [H25]. Buttons still act locally (K65 controller role); the event is informational |
| Firmware update (K93) | `update` per speaker | defaults to `config` [H24] | `UpdateEntityFeature.INSTALL | PROGRESS | RELEASE_NOTES`; **not** `SPECIFIC_VERSION` (so `update.install` cannot name an arbitrary version) and **not** `BACKUP`; `auto_update = False` (skip allowed) [H24]; HA refuses a second install while `in_progress` [H24]. `latest_version` = the server-staged, verified image only |
| Identify, resync | `button` | `config` / `diagnostic` | Speaker device |
| Satellite (K71) | `assist_satellite` per room | none | Plus the pipeline and VAD `select`s it references (as ESPHome does [H17]) |

K93 constraint: HA has no per-entity permission model, so "owner approves each install" in HA
means "anyone logged into HA" (K40, K85). **Proposed:** keep INSTALL (K93 says HA shows the update
and a press installs), but behind an integration option "Allow firmware install from Home
Assistant" (default **on** at home, documented), with A/B rollback as the safety net. The install
action carries no URL or version; the server installs the image it already holds.

---

## 6. K46: MQTT discovery beside the integration

Facts: HA's MQTT integration has no `media_player` platform [L4 §4]; with the integration, every
entity K46's MQTT table listed (volume, mute, source, group, EQ, diagnostics, update, buttons,
events) is an integration entity (sections 2 and 5). MQTT identifiers are namespaced to MQTT; a
shared MAC `connection` can merge the devices [H30] but the entities would still be
duplicates. The homelab has no MQTT consumer other than HA and Frigate [L2].

| Option | For | Against |
|---|---|---|
| Keep (full discovery) | Works if the integration is absent | Duplicates every entity; a second control path into chorus with its own ACL and validation work |
| **Limit** (deferred, optional, off by default; refused at start-up when the integration is connected, or simply documented as mutually exclusive) | Keeps K46's "interface for tools outside HA" promise for outside users (K9 OSS quality) | Still code to write later |
| Drop | Least code, smallest attack surface | Loses a generic interface; amends K46 |

**Recommendation R6: Limit**, and it is not in the first HA goal. What MQTT should carry if built:
non-discovery state and event topics (`chorus/<room>/state`, `chorus/<speaker>/button`) for
non-HA tools, plus discovery only in a "no integration" mode. Nothing that the integration already
exposes, at the same time. This is an amendment the owner should confirm (K46 and K61's wording
assumed MQTT would carry "what the integration does not"; the answer is "nothing, today").

---

## 7. K40: security requirements for the integration

HA is internet-reachable and anyone logged into HA can operate every enabled entity [L2]. The
integration runs inside HA's process with HA's privileges and reaches an unauthenticated control
API [L4 §5][L3]. Requirements (each should become a test):

1. **R7.1 Clamps on the server.** Per-room maximum volume and quiet-hours caps (K81) are enforced by
   chorus-server and the endpoint; a `volume_set 1.0` from HA is clamped, never trusted. The group
   volume scaling respects each member's cap. Announcement volume (`extra.volume`) is clamped too.
2. **R7.2 No arbitrary fetch.** `play_media` accepts media-source IDs (resolved inside HA),
   `chorus://input/*`, and URLs whose origin equals HA's configured internal or external URL.
   chorus-server holds the same allowlist (HA origin set at integration setup) and refuses any
   other fetch, so a compromised HA account cannot turn chorus into an SSRF probe on the IoT VLAN.
   No redirects followed off-origin; size and duration caps on fetched media.
3. **R7.3 OTA only for server-held, verified images.** The `update` entity has no
   `SPECIFIC_VERSION`; the install command names a speaker, not an image; the server checks
   signature/hash before staging; A/B with rollback (K93); install gated by an integration option.
4. **R7.4 Mic audio only to the Assist pipeline.** The voice route streams only during a pipeline
   run started by a server-side wake word or by `start_conversation`/`ask_question`; never
   offered as a playable source; hardware mute (K67) wins; each run has a hard time limit; the
   room LED shows listening (**proposed**).
5. **R7.5 start_conversation/ask_question open the mic.** Rate-limit them per room and log them;
   entity exposure to Assist/MCP stays off unless added to the homelab `registry.json` allowlist
   (`expose_new: false` today) [L2][L4 §5].
6. **R7.6 Input validation and translated errors.** Every action validates entity IDs, ranges and
   enums before sending a command (`action-exceptions`, `exception-translations`); the server
   still validates (426 refusals [L3]).
7. **R7.7 No credentials to leak today**, and `diagnostics` redacts hosts, MACs and the HA URL
   anyway (`async_redact_data`), since diagnostics files get pasted into issues.
8. **R7.8 Frontend code (U2) is served by the integration and registered explicitly** in the
   homelab YAML, never auto-injected (section 4).
9. **R7.9 No runtime pip** (`requirements: []`, vendored client, section 1.1).
10. **R7.10 Bounded resource use in HA.** Telemetry rate limits (1.2), SSE reconnect with backoff
    and jitter, one subscriber per config entry, `parallel-updates` set.
11. **R7.11 If chorus later gains auth** (K40 said no; revisit), the integration gains
    `reauthentication-flow` and the token lives in the config entry, never in YAML.

---

## 8. Decision table

| Decision | Options | Recommendation | Confidence | What would change it |
|---|---|---|---|---|
| K61 quality bar | Bronze / Silver / Gold / Platinum (self-certified) | **Platinum rules, `quality_scale.yaml`, machine-checked by running hassfest inside a core checkout**; `dependency-transparency` and `reauthentication-flow` exempt with reasons | High | HA changing hassfest's core/custom split; the owner accepting Gold to save effort (strict typing is the main Platinum cost) |
| K61 tests | PHCC / HA core checkout tests / none | **PHCC 0.13.366 (HA 2026.9.3), MIT, >95% coverage, shared JSON fixtures with the Rust server** | High | PHCC stopping daily releases (fall back to a core checkout) |
| K61 transport | HTTP+SSE / WebSocket / ESPHome API | **HTTP+SSE (existing) + one binary streaming route for voice audio** | Medium-high | Measured command latency or SSE reconnect gaps that HA users notice; a need for HA-to-server streaming |
| K61 discovery | zeroconf / none / manual only | **zeroconf `_chorus-ctl._tcp` + manual host as primary; add `id=` to TXT** | High | mDNS reflector removed from the homelab |
| K61 install | Manual copy / vendored read-only bind / install script / HACS | **Vendored in homelab git + read-only bind + lock line, via chorus-opened homelab PR** | Medium | Owner preferring not to grow homelab compose; read-only bind causing an HA loader problem (test first) |
| K61 client library | PyPI dependency / vendored package | **Vendored `aiochorus`, no runtime requirements** | High | Repo going public (then PyPI, K42) |
| K59/K54 room entities | per speaker (Sonos) / per room | **Per room, GROUPING, leader-first `group_members`** | High | Owner wanting per-speaker control in HA (bonded sets say no) |
| K59 saved groups | entity without GROUPING / MA-style with GROUPING / none | **Always-present entity, no GROUPING, group volume, `rooms` attribute** | Medium | Owner wanting to edit saved-group membership from HA's dialog (then MA-style G2) |
| K59 live groups | `group_members` only / dynamic entities | **`group_members` + per-room group-volume `number`** | Medium-high | HA adding a group-volume concept to media_player |
| K77 group volume | per room number / group entity volume / both | **Both: saved-group entity volume and per-room number for live groups** | Medium | A frontend group-volume feature landing in HA |
| K78 join semantics | take the room / skip busy | **Take the room on `join`; coalesced `unjoin`; leader hand-off** | High | (K78 already decided) |
| Decoding for play_media/TTS | MP3 decoder in server / force FLAC via tts options only | **MP3 (+WAV/FLAC) decoder in chorus-server; request FLAC/WAV via satellite `tts_options` where possible** | High | Owner forbidding codecs in chorus (then only satellite announcements work) |
| K71 voice | (a) own assist_satellite / (b) ESPHome-API emulation | **(a)** | Medium-high | HA deprecating custom access to `AssistSatelliteEntity`; the owner wanting voice without the chorus integration |
| K73 fit | server wake word + STT start / HA wake word stage | **Server wake word, pipeline at STT with `wake_word_phrase`** | High | (verified against ESPHome's own path) |
| K84 dashboard | stock / custom tile features / full card / iframe | **Stock now, custom tile features later; no full card** | Medium | Owner wanting drag-to-group in HA |
| K84 delivery | `add_extra_js_url` / static path + explicit YAML resource | **Static path served by the integration, registered in homelab YAML** | High | Homelab moving off YAML resource mode |
| K83 entities | table in section 5 | **As section 5; input select folded into media_player source** | Medium-high | Owner wanting quiet-hours schedules editable in HA |
| K93 install in HA | allowed / option-gated / read-only in HA | **Option-gated, default on, no SPECIFIC_VERSION** | Medium | HA remaining internet-reachable with weak accounts (then default off) |
| K46 MQTT | keep / limit / drop | **Limit: deferred, off by default, never beside the integration** | Medium-high | A non-HA MQTT consumer appearing in the house |
| K40 requirements | section 7 | **R7.1-R7.11 as testable requirements** | High | chorus gaining its own auth |

---

## 9. Sources and K33 log (all read 2026-09-29 unless marked ASSUMED)

| Tag | Source |
|---|---|
| H1 | https://developers.home-assistant.io/docs/core/integration-quality-scale/ (tiers; custom integrations not reviewed) |
| H2 | https://developers.home-assistant.io/docs/core/integration-quality-scale/rules (rule list by tier) |
| H3 | https://raw.githubusercontent.com/home-assistant/core/2026.9.3/script/hassfest/quality_scale.py (Rule table; `validate_iqs_file` returns when `not integration.core`) |
| H4 | https://raw.githubusercontent.com/home-assistant/core/2026.9.3/script/hassfest/__main__.py, `model.py` (`core` = path under `homeassistant/components`), `manifest.py` (`version` required for custom) |
| H5 | https://pypi.org/pypi/pytest-homeassistant-custom-component/json and `/0.13.364/json`, `/0.13.365/json`, `/0.13.366/json` (MIT; Python >= 3.14; HA core version badge per release; upload dates) |
| H6 | https://github.com/home-assistant/developers.home-assistant (raw `docs/core/integration-quality-scale/rules/{dependency-transparency,brands,strict-typing,test-coverage,discovery}.md`, `docs/creating_component_code_review.md`) |
| H7 | https://developers.home-assistant.io/blog/2026/02/24/brands-proxy-api (raw blog markdown) |
| H8 | https://developers.home-assistant.io/docs/creating_integration_manifest (raw markdown: version, zeroconf, custom requirements) |
| H9 | https://raw.githubusercontent.com/home-assistant/core/2026.9.3/homeassistant/components/sonos/{media_player.py,manifest.json,speaker.py} |
| H10 | https://raw.githubusercontent.com/home-assistant/core/2026.9.3/homeassistant/components/music_assistant/media_player.py |
| H11 | https://raw.githubusercontent.com/home-assistant/core/2026.9.3/homeassistant/components/media_player/__init__.py |
| H12 | https://developers.home-assistant.io/docs/core/entity/media-player (raw markdown) |
| H13 | https://github.com/home-assistant/frontend `dev`: `src/components/media-player/dialog-join-media-players.ts`, `src/dialogs/more-info/controls/more-info-media_player.ts`, `LICENSE.md` (Apache-2.0) |
| H14 | https://github.com/home-assistant/home-assistant.io `current`: `source/dashboards/features.markdown`; release posts 2025-01-03 (2025.1), 2025-06-11 (2025.6), 2025-09-03 (2025.9), 2025-10-01 (2025.10), 2026-05-06 (2026.5), 2026-06-03 (2026.6) |
| H15 | https://raw.githubusercontent.com/home-assistant/core/2026.9.3/homeassistant/components/tts/{__init__.py,const.py,media_source.py} |
| H16 | https://raw.githubusercontent.com/home-assistant/core/2026.9.3/homeassistant/components/assist_satellite/{entity.py,const.py,__init__.py} |
| H17 | https://raw.githubusercontent.com/home-assistant/core/2026.9.3/homeassistant/components/esphome/{assist_satellite.py,manifest.json} |
| H18 | https://raw.githubusercontent.com/home-assistant/core/2026.9.3/homeassistant/components/{wyoming,voip}/assist_satellite.py |
| H19 | https://raw.githubusercontent.com/home-assistant/core/2026.9.3/homeassistant/components/intent/timers.py (and `intent/__init__.py` exports) |
| H20 | https://raw.githubusercontent.com/home-assistant/core/2026.9.3/homeassistant/components/assist_pipeline/pipeline.py (`continue_conversation`) |
| H21 | https://developers.home-assistant.io/blog/2024/10/01/assist-satellite-entity (raw blog markdown) |
| H22 | https://raw.githubusercontent.com/esphome/esphome/dev/esphome/components/api/api.proto (MIT `.proto`; voice, capabilities messages) |
| H23 | https://crates.io/api/v1/crates/esphome-native-api and https://docs.rs/crate/esphome-native-api/latest |
| H24 | https://developers.home-assistant.io/docs/core/entity/update (raw markdown) + core 2026.9.3 `components/update/__init__.py` |
| H25 | https://developers.home-assistant.io/docs/core/entity/event (raw markdown) + core 2026.9.3 `components/event/__init__.py` |
| H26 | https://raw.githubusercontent.com/home-assistant/core/2026.9.3/homeassistant/components/frontend/__init__.py (`add_extra_js_url` docstring), `http/__init__.py` (`StaticPathConfig`) |
| H27 | developers.home-assistant `docs/frontend/custom-ui/registering-resources.md`; blog 2023-02-28 custom tile features; blog 2026-05-27 custom card suggestions |
| H28 | https://raw.githubusercontent.com/home-assistant/core/2026.9.3/homeassistant/components/media_source/helper.py (`content_filter`) |
| H29 | https://raw.githubusercontent.com/home-assistant/brands/master/README.md (custom brand images since 2026.3) |
| H30 | https://raw.githubusercontent.com/home-assistant/core/2026.9.3/homeassistant/requirements.py (`pip_kwargs`), `util/package.py` (uv install), `helpers/device_registry.py` (`get_entry` on identifiers or connections) |
| L1 | /cache/tmp/plan-2026-09-chorus/decisions.md rows K9, K28, K31, K40, K42, K46, K54, K56, K59, K61, K63-K65, K67, K71, K73, K77, K78, K81, K83-K85, K93, K95 |
| L2 | /cache/tmp/plan-2026-09-chorus/survey-homelab.md §2-§3; homelab-ro/docs/home-automation.md (code table, custom cards rule, borgmatic); homelab-ro/home-automation/homeassistant/docker-compose.yml (read-only binds) |
| L3 | /workspace/docs/control-plane.md (routes, SSE, bounded subscriber, discovery, no auth) |
| L5 | /cache/tmp/plan-2026-09-chorus/survey-chorus.md (crate table; "WebSocket replaced by HTTP + SSE with no ADR") |
| L4 | /cache/tmp/plan-2026-09-chorus/research-ha-ma-sources.md §1.4, §3-§6 (and its sources S34, S42) |

Not fetched (WebSearch quota exhausted): any 2026 HA frontend roadmap for a media group-volume
feature (none found in release notes 2025.1-2026.9 [H14]); HA's deprecation policy for custom
integrations using entity base classes (**ASSUMED**: the usual deprecation period applies).

**K33 log.** No GPL source was opened. Opened: HA core 2026.9.3 files (Apache-2.0) listed above,
HA frontend `dev` files (Apache-2.0, LICENSE.md checked), developer and user docs (markdown), and
ESPHome's `api.proto` (MIT under ESPHome's split licence; no ESPHome C++ was opened). crates.io
and docs.rs metadata only for `esphome-native-api` (MIT); its source was not read.
