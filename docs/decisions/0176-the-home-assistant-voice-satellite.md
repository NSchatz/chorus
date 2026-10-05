# 0176: a room that has a microphone is one Home Assistant Assist satellite on the room's device, beside a `voice enabled` switch and a read-only `mic muted` sensor; a wake word the server heard opens a run whose audio is pulled into the pipeline at speech-to-text, the run is stopped as soon as speech-to-text is done, and a reply or an announcement goes through the server's `announce` and is over when the pushed state says its number is no longer playing

- Status: accepted, 2026-10-05. Extends 0138 (the Home Assistant integration); uses 0169 (the
  microphone intake), 0172 (the voice run and the run-scoped route) and 0175 (announcements
  and how they end) as they are.
- Decided by: the owner for the path (proposal P8, Option A, approved: "chorus's own
  integration provides an `assist_satellite` entity per voice room"), for the route's
  protection (2026-10-04, "Run-scoped route (Recommended)") and for the rule (K73, K67, I4);
  the task for the scope (the satellite, the switch, the sensor; wake to stream to reply;
  announce that returns after playback); this record for the cheap decisions: which rooms,
  the entity model, when the run is stopped, how the end of playback is known, what a refusal
  becomes, and how the test harness carries Home Assistant's Assist components.
- Recorded by: the owner's agent harness, in the pull request that adds this record.
- Implemented in: `integrations/homeassistant/custom_components/chorus/`
  (`assist_satellite.py`, `binary_sensor.py`, `switch.py`, `announce.py`, `entity.py`,
  `coordinator.py`, `const.py`, `media_player.py`, `_aiochorus/client.py`,
  `_aiochorus/commands.py`, `_aiochorus/models.py`, `_aiochorus/errors.py`, `manifest.json`,
  `strings.json`, `icons.json`, `quality_scale.yaml`),
  `integrations/homeassistant/tests/test_assist_satellite.py`,
  `tests/aiochorus/test_voice.py`, `tests/fake_server.py`, `tests/test_live_server.py`,
  `tests/conftest.py`, `integrations/homeassistant/pyproject.toml`, `uv.lock`,
  `integrations/homeassistant/README.md`, `docs/home-assistant.md`

## Context

The server already does its half (0169, 0172): it keeps a voice room's microphone audio
only while the room has `voice_enabled` and the speaker's gate is live, hears the wake word
itself, publishes a `voice_wake` with the room and the phrase and nothing else, opens a run
on `voice_start` and answers with the run's identifier, and serves that run's audio on
`GET /api/voice-audio` to one reader, once, from one address, for at most the run's limit.
Announcements are numbered and listed in the state until they are over (0175, "How it
ended"). No server change was needed and none is made.

Home Assistant's `AssistSatelliteEntity` (core 2026.9.3) asks an integration for four
things: `async_accept_pipeline_from_satellite` with an audio stream, `on_pipeline_event`,
`async_announce`, which "should block until the announcement is done playing", and a
configuration of wake words.

## What was read

- `docs/proposals/P8-voice-path.md` (Option A and its re-verification table);
  `docs/control-plane.md`, "Voice: `voice_enabled` and `mic_muted`", "Voice: the wake word,
  the run and its audio" and "Announcements: the `announce` command"; `docs/decisions/0169`,
  `0172`, `0175`; `crates/server/src/control.rs` for the order of the refusals and the shape
  of the audio route's error body.
- Home Assistant core 2026.9.3 (Apache-2.0), as installed by the pinned harness:
  `homeassistant/components/assist_satellite/entity.py`, `const.py`, `__init__.py`,
  `services.yaml`; `assist_pipeline/__init__.py` (`async_pipeline_from_audio_stream`); the
  `manifest.json` of `assist_satellite`, `assist_pipeline`, `conversation`, `tts`, `stt`,
  `ffmpeg`, `intent`, `wake_word`; `tts/__init__.py` and `ffmpeg/__init__.py` for what they
  do at setup.
- No GPL source was opened. ESPHome's satellite was not read; what P8 records of it was
  enough.

## Decision

1. **Which rooms.** A room is a voice room while a speaker adopted into it lists `voice` in
   `speakers[].roles`, present or not: the catalog's own answer to "which rooms have a
   microphone at all". `mic_muted` cannot say it, since a room with no microphone reads
   muted too.
2. **Three entities, on the room's device, for voice rooms only.** The satellite, the Voice
   enabled `switch` (configuration) and the Mic muted `binary_sensor` (diagnostic). They
   come and go with the room's microphone, in the registry too. A room without a microphone
   has none of the three: a voice switch there would switch nothing a person can hear.
3. **The sensor is a sensor.** `mic_muted` is the speaker's hardware switch (K67). A binary
   sensor has no action, which is exactly what "the hardware state always wins; no HA action
   can unmute" needs. On means muted, as the catalog's field does.
4. **The stream of wake words is opened by the first satellite**, once per entry, and held
   like the other streams. A server with no voice room is never asked for it. While it is
   lost the satellites stay available (an announcement needs no wake word) and the log says
   so once.
5. **A wake word starts a run only if the room listens.** The satellite checks the last
   state's `voice_enabled` and `mic_muted` before it sends anything; the server's own
   refusal by name (`voice-disabled`, `mic-muted`) is treated the same when the state was
   stale. Neither is an error to a person: nothing was asked for.
6. **The pipeline starts at speech-to-text with the phrase**, which is Home Assistant's path
   for a wake word detected before the pipeline (P8's table). The audio is the route's raw
   16 kHz mono 16-bit PCM, which is what the base class declares to speech-to-text, so
   nothing is resampled. The client hands on whole samples only.
7. **The run is stopped at the first of**: the pipeline's `stt-end`, its `error`, its
   `run-end`, an exception out of the pipeline, a cancellation, the entity's removal. One
   `voice_stop` per run. Stopping at `stt-end` rather than at the pipeline's end closes the
   microphone while Home Assistant thinks and speaks. The server's limit remains the bound
   that does not depend on this code, and the client stops reading on its own two seconds
   past it (ASSUMED margin), on a monotonic clock.
8. **Playback is over when the state says so.** The `announce` answer's `announcement`
   number is looked up in each pushed state until it is no longer `playing`; a number the
   state no longer lists is over too (it left the last eight, or the server restarted). The
   wait is bounded at 660 s (ASSUMED: the server's ten-minute cut and a minute). Nothing is
   polled. `failed` raises a translated error with the server's reason; `displaced` is an
   end. The chime Home Assistant asks for before an announcement is one more announcement,
   waited for first; if it fails the announcement still plays.
9. **Announcements are not gated by the microphone.** They share the media player's path
   (`announce.py`), so the origin rule is in one place.
10. **A server that serves no run to this Home Assistant is a repair issue**, not a log
    line: `no-voice-integration` from `voice_start`, or `not-the-voice-integration` from
    the route. Both name `--voice-integration`. The issue is removed when a run's audio is
    read.
11. **The run's identifier stays where the server put it.** It is in the `voice_run` answer
    and in one request line. `VoiceRun` leaves it out of its `repr`, and the client's errors
    for the route never quote the request's address.
12. **Wake-word choice is not offered.** The base class makes the configuration methods
    abstract, so the satellite lists the server's models, all active, and refuses a change
    with a translated error. Choosing is the next voice task's.
13. **The manifest depends on `assist_pipeline` and `assist_satellite`** (as core's own
    satellites do) and still lists no requirement. The harness does not bring what those
    components import, so `hassil`, `home-assistant-intents`, `gazetteer-matcher`,
    `pymicro-vad`, `pyspeex-noise`, `mutagen` and `ha-ffmpeg` join the `dev` group at the
    versions core 2026.9.3's manifests pin, hash-locked. The harness pin itself does not
    move. They are what those components need to be imported and set up, which is a
    requirement of testing the platform at all.

## Not chosen

- **A voice switch and a mic sensor in every room.** Every room without a microphone would
  show a switch that does nothing. The live-server test would have been simpler (it
  could have driven the switch through the registry), which is not a reason to show a person
  a dead control; the test drives the same entity class against the real server instead.
- **Stopping the run only at the pipeline's end.** Simpler by one event, and the microphone
  would stay open through intent handling and the reply.
- **Sleeping for the clip's length**, or polling `GET /api/state`, to know when playback is
  over. The integration does not know the length (the server fetches the clip), and the
  state is pushed.
- **Making the media player's `play_media` announce wait as well.** `play_media` is not
  specified to block and its callers do not expect ten seconds of it. Left as it is.
- **Marking the satellite unavailable while the stream of wake words is lost**, as the
  button entities are. An unavailable satellite cannot announce, and announcing does not
  need the stream.
- **`START_CONVERSATION`, timers, continue conversation, ask question.** The next voice
  task's, by the task's own scope.

## Consequences

- Every setup of the integration now loads Home Assistant's Assist components. On a running
  Home Assistant they are part of `default_config`; a minimal installation without them
  gains them as dependencies.
- An automation that calls `assist_satellite.announce` waits for the clip, up to the
  server's ten-minute cut.
- A wake word said while the room speaks a reply is heard through the room's own sound:
  there is no echo cancellation (the mic hardware and AEC are the buy-list item's, K21).
- The server must be started with `--voice-integration <Home Assistant's address>`; with
  Home Assistant in a container the address is the one the server sees, and the repair issue
  says when it is not.
- The 660 s wait and the two-second read margin are ASSUMED, not measured; both are bounds
  on waiting, not timing claims about audio.
