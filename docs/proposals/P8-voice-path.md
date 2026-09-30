# P8: The voice path into Home Assistant

- Decisions: K71
- Status: PROPOSED (chorus goal 1, 2026-09-30); decided at Checkpoint K
- If deferred: The recommendation
- Builds on: goal 9 (§13 item 2, "the mic mute switch cuts the mic in firmware"), goal 18 (§22, the integration's shape, harness and "no unauthenticated endpoint"), goal 19 (§23, entities and the homelab install PR; §3.3 "voice satellites per P8"), goal 20 (§24 items 1-4 and done-when A-B: "each voice room as an HA Assist satellite by the settled path, tested against a fake pipeline"), goals 24 and 26 (the compact speaker's and the soundbar's mic with hardware mute, K67, K72)

## Question

How does a chorus room with a microphone become a Home Assistant voice satellite? K71: "goal 1
compares (a) chorus's own HA integration providing an assist_satellite entity per room (mic audio
speaker -> chorus-server -> HA Assist pipeline; replies/announcements through chorus's ducking
mixer) and (b) chorus-server emulating ESPHome-API voice devices (MIT Rust crate), on HA API
stability and features (timers, continue conversation, announce); decided at Checkpoint K. Not
chosen as final: either."

The owner decisions that bound it, quoted:

- K73: "On the server: speakers stream mic audio to chorus-server only while unmuted and voice is
  enabled; the server runs wake-word models with permissive licences (Apache-2.0 microWakeWord
  models; openWakeWord's bundled CC BY-NC-SA models excluded)."
- K67: "Buttons or touch, status LED, mic + hardware mute switch" (compact speaker); K72: the
  soundbar has a "mic with hardware mute".
- I4: "Speaker microphones feed only the voice path, never a shareable source".
- K61: "a chorus HA integration written to HA core's quality bar ... installed as a custom
  integration ... kept upstream-ready as a draft (K42)".
- K42: "upstream-bound code ... is written and tested inside chorus ... the program never opens a
  PR, issue or comment outside [the owner's] repos."
- K40: "chorus adds no auth of its own (trusted LAN) ... HA is internet-reachable, so chorus
  entities in HA are operable by anyone logged into HA."
- K31: "voice (chorus rooms as HA Assist satellites and announce targets)"; "announcements/ducking
  (TTS/notification over playback, duck and restore, HA-driven)".

## Constraints that bind every option

- **Clean-room** (K33): ESPHome's C++ runtime is GPLv3 ("The C++/runtime codebase of the ESPHome
  project (file extensions .c, .cpp, .h, .hpp, .tcc, .ino) are published under the GPLv3 license.
  The python codebase and all other parts of this codebase are published under the MIT license",
  ESPHome `LICENSE`). Its docs and the MIT `api.proto` may be read; `aioesphomeapi` (MIT) may be
  read; no ESPHome `.cpp`/`.h` is opened.
- **Licences** (K26, K95): chorus code MIT OR Apache-2.0; dependencies permissive or by ADR. HA core
  is Apache-2.0 (`LICENSE.md` at tag 2026.9.3).
- **Security** (brief §4.8): mic audio only to the voice path, cut by the hardware mute; the HA
  integration adds no unauthenticated endpoint to HA (`requires_auth=True`, `local_only`
  webhooks, tested).
- **Never touch the live HA** (brief §0.7): every test runs against fakes.
- **Hardware later** (K7, K21): the owner owns no mic; the mic buy-list item is goal 20's; AEC and
  mic hardware are outside this choice.
- CLAUDE.md rule 8: fitness for chorus's requirements only.

## Re-verification of the planning research

The planning research (`research-ha-integration.md` §3, 2026-09-29) recommended **(a)**, confidence
medium-high, superseding an earlier recommendation of (b) in `research-ha-ma-sources.md` §3 (V1,
"server-proxied ESPHome-API satellites"), which predated K61. Re-checked 2026-09-30:

| Planning claim | Re-check (primary source) | Changed? |
|---|---|---|
| `AssistSatelliteEntityFeature`: `ANNOUNCE = 1`, `START_CONVERSATION = 2` | HA core 2026.9.3 `assist_satellite/const.py:25-31`: the same | No |
| `async_accept_pipeline_from_satellite(audio_stream, start_stage=STT, end_stage=TTS, wake_word_phrase=None)`, STT fixed at 16 kHz 16-bit mono PCM | `assist_satellite/entity.py:438-446` (adds a keyword-only `context`), `:512-516` (`AudioFormats.WAV`, `AudioCodecs.PCM`, `BITRATE_16`, `SAMPLERATE_16000`, `CHANNEL_MONO`) | Minor: `context` added |
| Server-side wake word fits "start at STT with `wake_word_phrase`"; HA's intercept path says "Only on-device wake words currently supported" | `entity.py:454-479`: the same text; start at STT with a phrase is the accepted path | No |
| Timers via `intent.async_register_timer_handler(hass, device_id, handler)`, events started/updated/cancelled/finished | `intent/timers.py:159-171, 500-509`: the same | No |
| Core users: esphome (ANNOUNCE, START_CONVERSATION, timers, continue conversation), voip (ANNOUNCE, START_CONVERSATION, timers), wyoming (ANNOUNCE, timers) | 2026.9.3 `{esphome,voip,wyoming}/assist_satellite.py`: the same feature and timer use; only ESPHome forwards `continue_conversation` (`esphome/assist_satellite.py:384`) | No |
| Developer docs list the methods an integration implements | developers.home-assistant.io "Assist satellite entity": `async_accept_pipeline_from_satellite` ("Satellite entities should only run Assist pipelines using" it), `on_pipeline_event`, `async_get_configuration`, `async_set_configuration`, `async_announce`, `async_start_conversation`, `tts_response_finished`; properties `pipeline_entity_id`, `vad_sensitivity_entity_id`, `tts_options` | No |
| API history (planning had none) | Commit log of `assist_satellite/entity.py` up to tag 2026.9.3 (GitHub API): created 2024-09-06; two signature changes before its first release (#126299 "Change assist satellite announce method signature", 2024-09-20; #126926 "Change Assist satellite state names", 2024-09-27); introduced to developers 2024-10-01 (developer blog). Since then 20 commits, each subject additive: start_conversation (#134921, 2025-01-30), preannounce media (#141317, #141522, #141930, 2025-03), ask_question (#145233, 2025-06-19), satellite id through the pipeline (#151992, 2025-09-11), pipeline context (#179530, 2026-08-20) | New: two years without a breaking change to the subclass surface after release. Confirmed for signatures: a diff of the public classes' signatures in `entity.py` at tag 2024.10.0 against 2026.9.3 (goal-1 verifier 2) finds only additions (keyword-only `context`, `async_start_conversation`, `async_internal_start_conversation`, `async_internal_ask_question`, new optional parameters and `AssistSatelliteAnnouncement` fields); `async_announce(self, announcement)` and the pipeline method's positional parameters are unchanged. Behaviour still rests on goal 20's harness |
| `esphome-native-api` 3.0.0, MIT, majors 1.0/2.0/3.0 in 15 months, "work in progress", one protocol version, 14 stars | crates.io: 3.0.0 on 2026-07-15, 2.0.0 on 2025-12-29, 1.0.0 on 2025-04-27, all MIT, 26 releases, 10,787 downloads; GitHub: 14 stars, last push 2026-07-15; README: "This is still work in progress, so the API surface may change", "This crate only supports one ESPHome protocol version", "Support for multiple ESPHome versions via feature flags (not yet implemented)", "can be used for Server and Client implementations", encryption supported | No |
| `api.proto` "still moving" (capabilities migrating) | `api.proto` on `dev`: `DeviceCapabilitiesRequest/Response` for "api_version >= 1.15", and "Older clients keep reading DeviceInfoResponse, which still carries the same values, so this is not a breaking change"; 62 commits touched `api.proto` since 2025-09-30, including "Remove deprecated password authentication" (#12819, 2026-01-03) and "Add second audio channel for voice_assistant" (#16265, 2026-05-13) | Corrected: the capability change is additive; the protocol does remove deprecated features |
| HA's ESPHome client | HA 2026.9.3 `esphome/manifest.json` pins `aioesphomeapi==46.2.0` (MIT); PyPI shows five major versions (42 to 46) released since 2025-09-30 (42.0.0 on 2025-10-15 to 46.0.0 on 2026-08-23; 41.0.0 was 2025-09-15), newest 46.6.0 | New: HA's client ships a major about every two to three months |
| (b) needs a duplicate media_player per room for announcements | 2026.9.3 `esphome/assist_satellite.py:302-316`: ANNOUNCE comes from the device's voice-assistant feature flags; a media player is used for TTS only "if not (feature_flags & VoiceAssistantFeature.SPEAKER)" | Corrected: an emulated device that declares SPEAKER needs no media_player entity |
| Wyoming satellites are deprecated in favour of the ESPHome protocol | `OHF-Voice/linux-voice-assistant` (Apache-2.0, Open Home Foundation, pushed 2026-09-29): "Works with Home Assistant using the ESPHome protocol/API (via aioesphomeapi)", supports "announcments, start/continue conversation, and timers", "the satellite is automatically discovered by Home Assistant via the ESPHome integration". Primary for the deprecation: the archived `rhasspy/wyoming-satellite` README, "This project is no longer maintained as it has been replaced by Linux Voice Assistant that uses the ESPHome protocol" | New weight for (b): HA's own voice project ships a non-ESPHome satellite over the ESPHome API |
| Wake-word licences | GitHub API: `kahrendt/microWakeWord` Apache-2.0, `esphome/micro-wake-word-models` Apache-2.0 (models: `okay_nabu`, `hey_jarvis`, `alexa`, and a `v2` folder), `OHF-Voice/pymicro-wakeword` Apache-2.0; `dscripka/openWakeWord` code Apache-2.0 (its bundled models CC BY-NC-SA per K73, excluded) | No (goal 20 excludes trademarked words such as "alexa") |
| Test harness | PyPI `pytest-homeassistant-custom-component` 0.13.367, MIT (planning cited 0.13.366 for HA 2026.9.3) | Newer release exists; the pin follows homelab's HA |

Adversarially verified 2026-09-30 (goal-1 verifier 2): 16 claims confirmed, 1 refuted, 0 partly right, 0 unverifiable; corrections applied; the recommendation stands.

## Options

### Option A: chorus's own integration provides an `assist_satellite` entity per voice room (recommended)

- What: the chorus HA integration (K61, goals 18-19) gains an `assist_satellite` platform: one
  entity per room that has a mic, on the room's existing HA device. Mic audio: speaker to
  chorus-server over the encrypted speaker protocol (K62, K92), only while the hardware mute is off
  and voice is enabled (K73); chorus-server runs microWakeWord models; on a detection it tells the
  integration, which calls `async_accept_pipeline_from_satellite(stream, start_stage=STT,
  wake_word_phrase=...)` and feeds 16 kHz mono PCM pulled from one new streaming route on
  chorus-server for that run only. Replies (TTS) and `announce` arrive as HA media URLs, which the
  integration hands to chorus-server's ducking mixer for that room (goal 20 item 3); the entity
  returns from `async_announce` only after playback, as the base class requires. Timers through
  `async_register_timer_handler` (a timer sound and the LED from chorus); continue conversation by
  watching `on_pipeline_event` for `continue_conversation` and reopening the room's mic for a new
  run at STT; `start_conversation` and `ask_question` via the base class; wake-word choice through
  `async_get_configuration`/`async_set_configuration` listing the server's models.
- Mute semantics (K67): the hardware switch cuts the mic electrically and in firmware (goal 9);
  the speaker reports its state; the integration shows it as a read-only `binary_sensor` ("mic
  muted", a diagnostic of the room device) beside a `switch` "voice enabled" (K73's software
  gate). While muted or disabled: no pipeline starts; `announce` still plays (it uses the speaker,
  not the mic); `start_conversation` and `ask_question` play their prompt and then end the run
  without listening, raising a translated error so an automation sees why. The hardware state
  always wins; no HA action can unmute.
- Privacy (I4): mic audio exists in three places only: the speaker, chorus-server's wake-word
  buffer (memory only, never recorded, never a source), and the one pipeline run HA started.
  **One gap needs a decision:** K40 leaves chorus's control plane unauthenticated, so a plain
  streaming route on it could be read by any LAN client during a run. This proposal closes it
  without adding auth to the control plane: the route serves audio only for an active run, to a
  per-run identifier the server returns in the reply to the integration's own start command (never
  published on the SSE state stream), and only to the address the integration's config entry
  registered; the run has a hard time limit and the room LED shows listening. A pairing secret
  created in the config flow and pinned by the server on first use (the K92 pattern) is the
  stronger alternative; it is an owner call because it is a credential on the control API.
- Costs:
  - Money: none (the mic hardware is goal 20's buy-list line either way).
  - Effort: one Python module and its tests in the integration (goal 20), one streaming route and
    the run bookkeeping in chorus-server, the wake-word runner (both options need it). Roughly the
    smaller half of goal 20.
  - Maintenance: the integration is already pinned and tested per HA release (goal 18's harness
    moves with homelab's HA pin); the satellite platform rides that.
  - Owner steps: none beyond the integration's install (already a per-release Needs item).
- Risks:
  - HA's `AssistSatelliteEntity` is an internal Python API. Its record since the 2024.10
    introduction is additive (20 commits; a signature diff of 2024.10.0 against 2026.9.3 finds
    only additions), and three core
    integrations use it, but HA may change it; chorus sees that in its pinned test harness before
    the homelab's HA moves.
  - The mic route's protection is chorus-designed (above) rather than inherited from a protocol.
- Fit: one HA device per room carrying media_player, satellite and the chorus entities; announce
  and TTS go straight into chorus's mixer with room and group awareness (K31); server-side wake word
  is HA's own "already detected" path; the integration is the single upstream-ready draft (K42,
  K61).

### Option B: chorus-server emulates one ESPHome native-API voice device per voice room

- What: chorus-server listens on one TCP port per voice room and speaks the ESPHome native API
  (protobuf frames, Noise encryption with a per-device pre-shared key; the handshake name is
  `Noise_NNpsk0_25519_ChaChaPoly_SHA256`, as HA's own client defines it (aioesphomeapi, MIT,
  `NOISE_PROTOCOL_NAME` in `_frame_helper/noise_encryption.py`), and the crate's README says it
  supports "encryption"; ESPHome removed password auth from `dev` on 2026-01-03, #12819), announcing
  each as `_esphomelib._tcp` or added by host and port. Each emulated
  device declares voice-assistant features (`API_AUDIO`, `SPEAKER`, `ANNOUNCE`,
  `START_CONVERSATION`, `TIMERS`); HA's platinum ESPHome integration then creates the
  `assist_satellite` itself. Wake word on the server maps to `VoiceAssistantRequest{start,
  wake_word_phrase}` without `USE_WAKE_WORD`; audio as `VoiceAssistantAudio`; announcements as
  `VoiceAssistantAnnounceRequest` into chorus's mixer; timers as
  `VoiceAssistantTimerEventResponse`; wake-word choice as `VoiceAssistantConfiguration*`. The
  server side comes from the MIT `esphome-native-api` crate or from chorus's own code generated
  from the MIT `api.proto` (the crate's one-version limit argues for the latter).
- Mute semantics (K67): HA sees the satellite only through ESPHome entities, so the emulated device
  also lists a `binary_sensor` for the hardware mute and a `switch` for "voice enabled"; while muted
  it simply sends no `start`; for a `start_conversation` announce it plays and ends. Equivalent
  behaviour, more protocol surface.
- Privacy (I4): stronger by construction on the HA link: the mic stream crosses an encrypted,
  key-authenticated Noise channel only HA holds the key for. The per-room keys are chorus-generated
  and pasted into HA by the owner (a Needs step per room, and a key custody question under K4's
  "never create ... API keys": they are device keys, like K92's, but the owner should say so).
- Costs:
  - Money: none.
  - Effort: a Rust ESPHome-API server in chorus-server (framing, Noise, hello, device info,
    capabilities for API 1.15 and later, entity listing, the voice messages, mDNS), one listener per
    room, plus the chorus integration anyway (K61) and its link to the ESPHome device; roughly all of
    goal 20 for the protocol alone.
  - Maintenance: tracks two moving parts: ESPHome's wire protocol (62 commits to `api.proto` in a
    year; additive with deprecation-then-removal, e.g. password auth) and HA's client
    (`aioesphomeapi`, five majors in a year, pinned per HA release).
  - Owner steps: add each room's device in HA's ESPHome integration with its key (8 rooms at K75's
    scale); homelab firewall and discovery for N extra host ports (HA runs on a bridge network, so
    discovery depends on the mDNS reflector or manual host and port).
- Risks:
  - Two HA devices per room (the chorus room device and the ESPHome device) unless the chorus
    integration registers the room device with the same network-MAC connection as the emulated
    device, which HA's device registry merges (**ASSUMED**, untested here); the emulated device
    needs a stable synthetic MAC.
  - Group and ducking awareness is indirect: HA's ESPHome integration knows nothing of chorus
    groups; chorus maps the device back to its room.
  - The crate is pre-1.0 in spirit (three majors in 15 months, "work in progress"), one protocol
    version per build.
- Fit: works even without chorus's integration, which K61 makes moot; follows the path HA's own
  voice project takes for non-ESPHome satellites (Linux Voice Assistant), a real stability signal;
  splits chorus's HA face across two integrations.

### The fallback

If deferred, later goals build on the recommendation (option A), and the finale lists P8 under
"Proposals awaiting the owner".

## Comparison

| Criterion | A: own `assist_satellite` | B: ESPHome-API emulation |
|---|---|---|
| API chorus depends on | HA's Python `AssistSatelliteEntity` and `intent` timers: internal, additive since 2024.10, three core users | ESPHome wire protocol as HA's `aioesphomeapi` reads it: versioned, backward compatible, removes deprecated features; HA's own voice project uses it for non-ESPHome satellites |
| Where a break shows up | chorus's pinned HA test harness, before homelab moves | a protocol or client change; chorus tests with `aioesphomeapi` pinned to HA's version |
| Announce (with pre-announce) | yes, straight to chorus's mixer | yes, via `VoiceAssistantAnnounceRequest` to chorus's mixer |
| Start conversation, ask question | yes (base class) | yes (declared feature, base class) |
| Timers | yes (`async_register_timer_handler`) | yes (`TIMERS` feature) |
| Continue conversation | yes (watch `continue_conversation`, reopen the mic) | yes (HA forwards it; the device reopens) |
| Server wake word (K73) | start at STT with `wake_word_phrase` | `VoiceAssistantRequest` with `wake_word_phrase` |
| Wake-word choice in HA | `async_get/set_configuration` | `VoiceAssistantConfiguration*` messages |
| Mute (K67) | chorus entities on the room device | emulated ESPHome entities |
| Mic link privacy (I4) | chorus-designed run-scoped route (a decision, above) | Noise-encrypted, key-authenticated |
| HA devices per room | one | two, or one if the MAC-connection merge works (ASSUMED) |
| Group and ducking awareness | direct | indirect |
| Server surface | one route | one listener per room, Noise, protobuf |
| Owner steps | none beyond the integration install | a key per room in HA; homelab ports |
| Testing against a fake HA | PHCC (MIT) with a faked `async_pipeline_from_audio_stream`; server route tested in Rust on shared fixtures | `aioesphomeapi` (MIT, HA's own client) against chorus-server; HA's ESPHome integration under PHCC |
| Upstream-readiness (K42, K61) | the satellite ships inside the one upstream-ready integration | nothing to upstream for voice; the integration stays smaller |
| Effort | smaller half of goal 20 | about all of goal 20 for the protocol |

## Recommendation

**Recommendation:** Option A, an `assist_satellite` entity per voice room in chorus's own integration, because it gives every feature K71 names with one HA device per room, puts announcements and replies straight into chorus's ducking mixer, and adds one server route instead of an ESPHome server per room.

Why: both paths reach the same HA features (timers, continue conversation, announce, start
conversation, ask question, wake-word choice) and both fit server-side wake word exactly; HA's own
code shows both. A is less code, fewer owner steps and one device per room, and it keeps chorus's HA
face in the single upstream-ready integration K61 asks for. The re-check strengthened B more than
the planning research allowed (no duplicate media_player is needed, the capability change is not
breaking, and HA's voice project itself uses the ESPHome protocol for Linux satellites), so the
margin is effort and cohesion, not feasibility. What the owner gives up: B's protocol-level
encryption of the HA mic link and its independence from chorus's integration. A needs one decision
with it: how the mic streaming route is protected on an otherwise unauthenticated control plane
(the run-scoped route above, or a pinned integration secret). If HA ever makes
`AssistSatelliteEntity` unavailable to custom integrations, B is the ready fallback, and the
wake-word runner, mixer and mute handling carry over unchanged.

## If the owner defers

Goals 18-20 build on the recommendation (option A) with the run-scoped mic route, and the program
report lists P8 under "Proposals awaiting the owner". The cost is small: the choice touches only
goal 20's integration module and one server route; everything else (the wake-word runner, the
mixer, mute handling, the fake-pipeline tests) is common to both options.

## Open inputs

- The mic route's protection: the run-scoped route (recommended) or a config-flow secret pinned by
  the server (an owner call, because K40 said chorus adds no auth).
- Whether B's per-room Noise keys count as "keys" under K4 (only if the owner picks B).
- HA's deprecation practice for custom integrations using entity base classes: **ASSUMED** the
  usual announced deprecation period; not re-read.
- That every `assist_satellite/entity.py` change since 2024.10 is additive: confirmed for the
  public signatures (a diff of tag 2024.10.0 against 2026.9.3, goal-1 verifier 2); behaviour is
  checked by goal 20's harness.
- The HA device-registry merge on a shared MAC connection (only B): **ASSUMED**.
- Mic hardware and AEC for voice rooms: goal 20's buy-list item (K21, K36).
- Spotify Developer Policy §III (voice), from P7 (`P7-spotify-soloist.md`, terms table): if the
  owner reads chorus's controller as a Spotify SDA, voice commands that reach a room whose source
  is a Soloist instance are in scope of "Do not create a voice-enabled SDA that enables a user to
  control Spotify with their voice, or any kind of voice assistant that provides voice-control
  functionality" (https://developer.spotify.com/policy, read 2026-09-30). The owner's call at
  Checkpoint K. Options A and B are affected equally, so the recommendation does not change; at
  minimum, no voice intent targets a Soloist source.

## Sources

- Home Assistant core, tag 2026.9.3 (Apache-2.0): `homeassistant/components/assist_satellite/{entity.py,const.py,__init__.py}`, `intent/timers.py`, `{esphome,voip,wyoming}/assist_satellite.py`, `esphome/manifest.json`, `LICENSE.md`, https://raw.githubusercontent.com/home-assistant/core/2026.9.3/homeassistant/components/assist_satellite/entity.py (and the sibling paths), read 2026-09-30
- Commit history of `assist_satellite/entity.py` at 2026.9.3, https://api.github.com/repos/home-assistant/core/commits?path=homeassistant/components/assist_satellite/entity.py&sha=2026.9.3, read 2026-09-30
- HA developer docs, "Assist satellite entity", https://developers.home-assistant.io/docs/core/entity/assist-satellite/, read 2026-09-30
- HA developer blog, "Introducing the Assist satellite entity" (2024-10-01), https://developers.home-assistant.io/blog/2024/10/01/assist-satellite-entity/ (search result), read 2026-09-30
- ESPHome `api.proto` (MIT under ESPHome's split licence), https://raw.githubusercontent.com/esphome/esphome/dev/esphome/components/api/api.proto, and its commit history since 2025-09-30, https://api.github.com/repos/esphome/esphome/commits?path=esphome/components/api/api.proto, read 2026-09-30
- ESPHome licence, https://github.com/esphome/esphome/blob/dev/LICENSE (via the GitHub licence API), read 2026-09-30
- `esphome-native-api` crate, https://crates.io/api/v1/crates/esphome-native-api and `/versions`; repository README https://github.com/UbiHome/esphome-native-api, read 2026-09-30
- `aioesphomeapi`, https://pypi.org/pypi/aioesphomeapi/json (46.6.0, MIT, release dates) and https://github.com/esphome/aioesphomeapi (licence), read 2026-09-30
- Linux Voice Assistant README, https://github.com/OHF-Voice/linux-voice-assistant (Apache-2.0), read 2026-09-30
- `wyoming-satellite` README (archived), https://github.com/rhasspy/wyoming-satellite, read 2026-09-30
- `aioesphomeapi` Noise protocol name (MIT), https://raw.githubusercontent.com/esphome/aioesphomeapi/main/aioesphomeapi/_frame_helper/noise_encryption.py, read 2026-09-30
- HA core `assist_satellite/entity.py` at tag 2024.10.0 (signature diff against 2026.9.3, goal-1 verifier 2), https://raw.githubusercontent.com/home-assistant/core/2024.10.0/homeassistant/components/assist_satellite/entity.py, read 2026-09-30
- Spotify Developer Policy §III (voice clause; see P7), https://developer.spotify.com/policy, read 2026-09-30
- Wake word licences via the GitHub API: https://github.com/kahrendt/microWakeWord, https://github.com/esphome/micro-wake-word-models (and its `models/` listing), https://github.com/OHF-Voice/pymicro-wakeword, https://github.com/dscripka/openWakeWord, read 2026-09-30
- `pytest-homeassistant-custom-component`, https://pypi.org/pypi/pytest-homeassistant-custom-component/json (0.13.367, MIT), read 2026-09-30
- `research-ha-integration.md` §0, §1.2-§1.4, §3, §7, §8, §9; `research-ha-ma-sources.md` §3-§5 (voice, MQTT and security facts only; its Music Assistant/Sendspin recommendation is declined by K56) (chorus planning research), read 2026-09-30

## What was read

chorus: `/workspace/.claude/goals/2026-09-chorus.md` (§0.1, §0.3, §0.7, §0.13, §1 with K and I rows,
§2, §3.3, §3.4, §4.8, §5, §13, §22-§24, §28-§31), the research files `research-ha-integration.md`,
`research-ha-ma-sources.md` (§3 voice, §4 MQTT, §5 security, §7 sources and K33 log only),
`/cache/tmp/chorus-g1/agent-rules.md`, `/cache/tmp/chorus-g1/proposal-format.md`. Web and APIs: the
URLs under Sources, plus two web searches (HA developer blog on the satellite entity; neither result
page beyond the listed ones was opened). Permissive source opened: HA core files (Apache-2.0) listed
above, ESPHome `api.proto` (MIT), the crate's and Linux Voice Assistant's READMEs. **No GPL source
was opened**: no ESPHome C++ (`voice_assistant`, `micro_wake_word`, `api` `.cpp`/`.h`), no Piper
(GPL), no source of any other GPL project; only ESPHome's `LICENSE` file and `.proto`. No live Home
Assistant, broker or device was touched. Corrections pass (2026-09-30): the goal-1 verifier 2
report (`/cache/tmp/chorus-g1/verify/verify-2.md`), PyPI's aioesphomeapi release dates, the
archived wyoming-satellite README, aioesphomeapi's `noise_encryption.py` (MIT, grep for the
protocol name) and the Spotify Developer Policy.
