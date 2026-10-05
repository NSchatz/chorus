# 0179: a room chooses which of the server's wake words it listens for with one catalog command, kept per room and persisted; the Home Assistant satellite sends that choice, continues a conversation with a second run and no wake word, plays a prompt and then opens a run for start conversation and ask question, and plays Home Assistant's chime three times when a timer finishes

- Status: accepted, 2026-10-05. Extends 0176 (the Home Assistant voice satellite), whose
  item 12 ("wake-word choice is not offered") and "Not chosen" entry for start conversation,
  timers, continue conversation and ask question it replaces; uses 0172 (the voice run) and
  0175 (announcements and how they end) as they are.
- Decided by: the owner for the path (proposal P8, Option A, approved: "Timers through
  `async_register_timer_handler`", "continue conversation by watching `on_pipeline_event`
  for `continue_conversation` and reopening the room's mic for a new run at STT",
  "`start_conversation` and `ask_question` via the base class", "wake-word choice through
  `async_get_configuration`/`async_set_configuration` listing the server's models", and the
  mute semantics: "play their prompt and then end the run without listening, raising a
  translated error"), and for the Spotify voice clause (2026-10-04, "Leave as built":
  chorus defines no voice intent that targets a Soloist source); the task for the scope,
  which allows "a small server or control-catalog addition only if selecting the wake word
  needs a command"; this record for the cheap decisions: the command's shape, what a room
  that never chose does, where the choice is kept, where a detection is dropped, what the
  timer sounds like, when a conversation is continued, and what a refusal becomes.
- Recorded by: the owner's agent harness, in the pull request that adds this record.
- Implemented in: `crates/control/src/catalog.rs` (`Command::VoiceWakeWords`,
  `is_wake_word_id`, `MAX_WAKE_WORDS`), `zones.rs` (`Zone::wake_words`,
  `Zones::listens_for`, the room's `wake_words` in the state), `persist.rs` (format 9);
  `crates/server/src/voice.rs` (`Voice::pass_where`), `control.rs` (`voice_pass`);
  `docs/control-plane.md`, `docs/mqtt.md`;
  `integrations/homeassistant/custom_components/chorus/` (`assist_satellite.py`,
  `_aiochorus/commands.py`, `_aiochorus/models.py`, `_aiochorus/client.py`,
  `manifest.json`, `strings.json`), `integrations/homeassistant/README.md`,
  `docs/home-assistant.md`; held by `crates/control/tests/voice_v2.rs`, `crates/server/tests/voice_run.rs`, the unit tests of
  `voice.rs`, the vectors `fixtures/control/v2/voice_wake_words`, `state-wake-words` and
  `error-voice-wake-words-unknown`, `integrations/homeassistant/tests/test_assist_satellite.py`
  and `tests/aiochorus/test_voice.py`.

## Context

0176 left the satellite with the basic run: a wake word, a pipeline, a reply, and
`announce`. Home Assistant's `AssistSatelliteEntity` (core 2026.9.3) offers more to an
integration that implements it, and P8 named each piece. One of them had nothing to send:
the server ran every wake-word model in every voice room and the catalog had no command to
say otherwise, so `async_set_configuration` could only refuse.

## What was read

- `docs/proposals/P8-voice-path.md` (Option A; the comparison table; the Spotify voice
  clause under "Open inputs"); `docs/decisions/0176`, `0172`, `0169`, `0167`;
  `docs/control-plane.md`, "Voice: the wake word, the run and its audio".
- Home Assistant core 2026.9.3 (Apache-2.0), as installed by the pinned harness:
  `assist_satellite/entity.py` (`async_internal_start_conversation`,
  `async_internal_ask_question`, `async_accept_pipeline_from_satellite`,
  `_internal_on_pipeline_event`), `assist_satellite/__init__.py` (the actions and the
  features they need), `assist_satellite/websocket_api.py` (`set_wake_words`),
  `assist_satellite/const.py` (`PREANNOUNCE_URL`), `intent/timers.py` (`TimerManager`,
  `async_register_timer_handler`, `TimerEventType`), `esphome/assist_satellite.py` (how a
  core satellite registers the timer handler, forwards `continue_conversation` and sets its
  configuration), and the `manifest.json` of `esphome` and `voip` (both list `intent`).
- No GPL source was opened.

## Decision

### The server

1. **One command, per room: `voice_wake_words`** with `zone` and `wake_words`, an array of
   at most 16 (ASSUMED: far past the models one build carries) ids of the state's
   `wake_words[].id`, none twice. Home Assistant's configuration is per satellite and a
   satellite is a room, so the choice is the room's.
2. **A room that never chose listens for every model**, which is what every room did before
   this record, so nothing changes for a house that sends no such command. The room's state
   carries `wake_words` only once it chose, after `mic_muted`: no existing state vector
   changes and an older consumer sees nothing new until the command is used. An empty array
   is a choice: the room answers to no wake word and a run can still be opened there by
   `voice_start`, which is what start conversation and ask question do.
3. **An id the server does not run is refused by name** (`unknown-wake-word`, field
   `wake_words`), naming the ones it runs. The shape of an id (lower-case letters, digits,
   underscores) is the decoder's to hold; which ids exist is the room model's, since the
   models are a fact about the build.
4. **The choice is persisted: state-file format 9**, `wake_words` in `[zone]`, `*` for a
   room that never chose and otherwise the ids, comma-separated, possibly none; required in
   a format 9 file as every format's own fields are. A format 1 to 8 file loads with `*`.
   A loaded id is held to its shape only: the list of models is set after the file is read
   and may differ in another build, and an id with no model matches nothing.
5. **Every model still runs on every voice room's audio; a detection is dropped where the
   room does not listen for it** (`Voice::pass_where`), before it becomes a wake word: no
   `voice_wake`, no hold-off, no "starts at the wake word" for a later run, no log line.
   Dropping it there, rather than where the event is fanned out, is what keeps a phrase the
   room does not answer to from holding off one it does. Not running a model at all for a
   room that does not want it would save its inference; with one model in the build that is
   nothing, and it would tie each session's detectors to a setting that can change under
   them.
6. **No command returns a room to "every one".** Naming every id says the same until the
   build gains a model. Left out as surface nothing asks for yet.

### The satellite

7. **Wake-word choice.** `async_get_configuration` lists the server's models and, as active,
   the room's choice (all of them for a room that never chose);
   `max_active_wake_words` is the number of models, because Home Assistant refuses a choice
   longer than it and 0 there refuses every choice. `async_set_configuration` sends the
   command; the refusal is a translated error with the server's words.
8. **Continue conversation.** `continue_conversation` is read at `intent-end`. When the
   reply has been played to its end the satellite opens a second run itself and starts a
   pipeline at speech-to-text with no wake word phrase; the base class carries the
   conversation over. The server's rule already fits: a `voice_start` with no wake word
   before it starts at the command. Nothing is continued when the reply could not be played
   (nobody heard the question), when the gate closed meanwhile, or when a wake word or a
   prompt started a run first.
9. **Start conversation and ask question** share `async_start_conversation`, as the base
   class has it. The prompt is an announcement (0176 item 8: it returns when the clip is
   over); then a run and its audio are opened in the call itself, so a room that does not
   listen is an error of the call and not a line in the log; then the pipeline runs in the
   background and the call returns, which is what `ask_question` needs to wait on its
   answer. A wake word during the prompt is ignored: the room is about to listen anyway.
10. **While muted or voice disabled the prompt plays and the call raises.** The gate is read
    from the last state after the prompt, and nothing is asked of the server; the server's
    own refusal by name, when the state was stale, becomes the same error. The three keys
    are `not_listening_voice_disabled`, `not_listening_mic_muted` and `not_listening` (any
    other reason, quoted).
11. **A question cannot wait for ever.** The base class waits on a future that only a
    pipeline event resolves. A pipeline that ends before its first event (Home Assistant
    raised while starting it) sends none, so the run that listens for an answer resolves the
    question with no answer when it is over. This reads one attribute of the base class
    (`_ask_question_future`), which the pinned harness holds still.
12. **Timers.** The handler is registered for the room's device, which is what makes Home
    Assistant accept a voice timer there. `finished` plays Home Assistant's own chime
    (`/api/assist_satellite/static/preannounce.mp3`) three times through `announce`
    (ASSUMED: three is enough to be noticed and short enough to need no "stop"); two timers
    that end together ring once; the other events are not shown.
13. **The manifest lists `intent`**, as core's `esphome` and `voip` do for the same import.

## Not chosen

- **A timer sound of chorus's own.** The server fetches a clip only from Home Assistant's
  origin, without credentials, so the integration would have to register a static route that
  asks for none. It has no such route today
  (`tests/test_no_unauthenticated_endpoint.py`), and one sound is not a reason to add the
  first. The chime is served by Home Assistant itself, on every installation.
- **A timer that rings until stopped.** It needs a stop the room can hear, and a wake word
  said over the room's own sound is not reliable without echo cancellation (K21).
- **A light on the speaker for a running timer** (P8: "a timer sound and the LED from
  chorus"). No command of the server lights a speaker for a voice event; that is firmware
  and protocol work outside this task's scope. The README says so.
- **Keeping the choice in Home Assistant** and sending it again after each server restart.
  The server is where the wake word is heard; a choice it forgets is a room that answers
  to the wrong word until Home Assistant notices.
- **Continuing after a reply that failed to play**, or without waiting for the reply. The
  person would be listened to before, or without, hearing what was asked.
- **Checking the gate before the prompt and not playing it.** P8's mute semantics say the
  prompt is played: an announcement uses the speaker.
- **Per-satellite pipeline and sensitivity select entities.** Not in the task; the README
  lists them as not supported.

## Consequences

- A state file written by this build is format 9 and is refused by an older build, as every
  format bump is.
- The MQTT room payload gains `wake_words` for a room that chose, being the room's object
  byte for byte.
- An automation that calls `start_conversation` or `ask_question` on a muted or disabled
  room gets an error after the prompt has been heard.
- With one model in the build the choice is between "Okay Nabu" and no wake word; the
  mechanism is in place for the next model.
- The three-chime timer sound and the 16-id bound are ASSUMED, not measured; neither is a
  timing claim about audio.
