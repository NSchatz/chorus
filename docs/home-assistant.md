# chorus and Home Assistant

The integration lives in `integrations/homeassistant/custom_components/chorus` (goal 18). This
document is how it maps chorus to Home Assistant, the security rules it is held to, its
test harness, how it is installed (a pinned copy, never HACS) and what changes when it is
submitted to Home Assistant core. What a person installing it reads is `integrations/homeassistant/README.md`; why
it is shaped this way is `docs/decisions/0138-the-home-assistant-integration.md`.

## The shape

- Domain `chorus`, a hub, `local_push`, set up by a config flow, discovered by zeroconf
  (`_chorus-ctl._tcp.local.`).
- Transport: the control plane as it is (`docs/control-plane.md`). One event-stream
  subscriber per config entry; commands by `POST /api/command` with
  `Content-Type: application/json` and no `Origin` header. The state is never polled. The
  speakers' diagnostic sensors alone are, from `GET /metrics`, once a minute and only while
  one of them is enabled ("Speaker diagnostics" below).
- **No runtime requirement.** The client library is vendored at
  `custom_components/chorus/_aiochorus/`: async only, fully typed (`py.typed`), no Home
  Assistant import, an injected `aiohttp.ClientSession`. That directory is its one source
  location. The manifest's `requirements` is `[]`, so Home Assistant never installs a package
  for chorus.
- Written to every rule of the Bronze, Silver, Gold and Platinum tiers of Home Assistant's
  Integration Quality Scale as core 2026.9.3 lists them (54 rules), self-certified in
  `quality_scale.yaml`: 47 done, 7 exempt, each exemption with its reason. A custom
  integration cannot hold a tier; `make ha-hassfest` has Home Assistant's own hassfest grade
  the file.

## The server's identity

`GET /api/server` answers
`{"v":2,"t":"server","id":"<id>","software":"<version>","catalogs":[1,2],"announce_origins":[...]}`.
`id` is the config entry's unique id (and the `id=` of the advertisement's TXT record), so a
server cannot be set up twice and a discovered server that moved updates its entry. A server
whose `catalogs` lack 2, that has no such route, or that answers a command `refused` (426), is
a translated error in the flow and a repair issue at run time. The id is opaque to the
integration (`chorus-server-` and 16 hex digits, derived from the server's key); a server
started with `--ephemeral-identity` has a new one at every start, so it is a new server to
Home Assistant each time.

## What becomes what

| chorus | Home Assistant |
|---|---|
| The server | One device (a service), the hub of the entry |
| A room (zone) | One device named for the room, `suggested_area` its name, with a `media_player`, a group-volume `number`, a visualizer `sensor` (disabled by default), and its sound controls: bass and treble (`number`), loudness, night mode and speech enhancement (`switch`), an input `select`, a quiet-hours `switch`, and an autoplay `switch` for each autoplay rule that targets it |
| A room that has a microphone | On the room's device as well: an `assist_satellite`, a Voice enabled `switch` and a Mic muted `binary_sensor` ("Voice rooms" below) |
| A saved group | One device under the server with a `media_player`, always present, and an autoplay `switch` for each autoplay rule that targets it |
| A live group | No entity: the `group_members` of its rooms' players, leader first |
| An adopted speaker | One device named for the speaker, `via` its room's device while it has a room and the server otherwise, a firmware `update` entity once it has reported what it runs, eight diagnostic `sensor`s, and five button `event` entities once it has declared the controller role |

Home Assistant's areas are Home Assistant's: the integration only suggests one and never
creates, renames or assigns an area.

Rooms, saved groups and speakers that appear while Home Assistant runs get their devices and
entities at once; ones that disappear (a forgotten speaker) have their devices removed; a
renamed room, group or speaker renames its device. A speaker's device exists whether or not it
has an entity, and follows its room: assigned to another room it hangs under that room's
device, unassigned under the server.

### Which command each action sends

| Home Assistant action | Catalog command (exact bytes are in `tests/test_media_player.py`) |
|---|---|
| room `volume_set` | `{"v":1,"t":"volume","zone":Z,"volume":0.500}` |
| room `volume_up` / `volume_down` | `volume_step`, `step` 50 or -50 |
| room `volume_mute` | `{"v":1,"t":"mute","zone":Z,"muted":true}` |
| room `join(group_members)` | for each member not already in the room's group: `{"v":2,"t":"join","zone":MEMBER,"target":ROOM}` |
| room `unjoin` | `{"v":2,"t":"take","target":ROOM}` (nothing when the room is already alone) |
| room or saved group `select_source`, `play_media chorus://input/...` | `{"v":2,"t":"take","target":T,"source":S}` |
| `turn_on` / `turn_off` | `take` with `source` `stream` / `none` |
| `play_media` with `announce` | `{"v":2,"t":"announce","target":T,"url":U}`, with `"volume":0.300` when `extra.volume` is given |
| `media_pause`, `media_play`, `media_next_track`, `media_previous_track` | `playback` with `pause`, `resume`, `next`, `previous` |
| saved group `volume_set` | `{"v":2,"t":"group_volume","group":G,"volume":0.400}` |
| saved group `volume_up` / `volume_down` | `group_volume_step`, `step` 50 or -50 |
| saved group `volume_mute` | `mute` for each of its rooms |
| the room's group-volume `number` | `group_volume` for the room's formed group |
| the room's Bass or Treble `number` | `{"v":2,"t":"sound","zone":Z,"bass":3}` (or `"treble"`): that field alone |
| the room's Loudness, Night mode or Speech enhancement `switch` | `{"v":2,"t":"sound","zone":Z,"night":true}` (or `"loudness"`, `"speech"`): that field alone |
| the room's Input `select` | `{"v":2,"t":"take","target":Z,"source":S}`, as `select_source` |
| the room's Quiet hours `switch` | `{"v":2,"t":"quiet_hours_enabled","zone":Z,"enabled":false}` |
| an Autoplay `switch` | `{"v":2,"t":"autoplay","input":I,"target":T,"enabled":false}`, with the rule's `stop_on_standby` and `low_latency` written again when they are `false`: the command replaces the rule |
| the room's Voice enabled `switch` | `{"v":2,"t":"voice_enabled","zone":Z,"enabled":true}` |
| a wake word heard in the room (no action of Home Assistant's) | `{"v":2,"t":"voice_start","zone":Z}`, then `GET /api/voice-audio?run=<the answer's identifier>`, then `{"v":2,"t":"voice_stop","zone":Z}` |
| the satellite's wake words, set in Home Assistant's voice settings | `{"v":2,"t":"voice_wake_words","zone":Z,"wake_words":["okay_nabu"]}` |
| a reply that asks for more, `assist_satellite.start_conversation` and `assist_satellite.ask_question` | `announce` for the reply or the prompt, waited for, then `voice_start`, the run's audio and `voice_stop` as after a wake word |
| a timer of the room's device that finishes | `{"v":2,"t":"announce","target":Z,"url":<Home Assistant's chime>}`, three times, each waited for |
| the room's satellite, `assist_satellite.announce`, and a pipeline's reply | `{"v":2,"t":"announce","target":Z,"url":U}`, once for the chime and once for the clip, each waited for |
| a speaker's Firmware `update`, `update.install` | `{"v":2,"t":"firmware_install","speaker":S,"image":I}`: that speaker, the verified staged image, never `all`, never `force` |

The sound controls' bytes are in `tests/test_sound_controls.py`, each held to the vector in
`fixtures/control/v2/` (`sound.json`, `sound-partial.json`, `take-source.json`,
`quiet_hours_enabled.json`, `autoplay.json`); the entity model is
`docs/decisions/0152-the-home-assistant-sound-controls.md`. The install's bytes are in
`tests/test_update.py`, held to `firmware_install.json`.

A volume is Home Assistant's 0..1 level as the catalog's amplitude factor, written with
exactly three decimals. The server clamps every volume to the room's limits; the entity shows
what the state says afterwards, never what was asked.

**Join is "take the room"** (K78): the member leaves whatever it played. `join` with a room
as its `target` is the catalog command whose documented meaning is exactly that, and it forms
a live group when the leader was alone. **Unjoin is `take` on the room**: the catalog says
`take` moves the room into the group named for it and dissolves a live group left with one
room. Catalog v1's `ungroup` also moves the room but "dissolves nothing", which would leave a
one-room live group behind; `join` has no form that means "leave".

## Speakers and firmware: nothing installs without the install action

The model is `docs/decisions/0154-the-home-assistant-speaker-devices-and-firmware-updates.md`;
the server's side is `docs/firmware-updates.md` and `docs/control-plane.md`, "Firmware: staged
images and explicit installs". K93 and I13 hold here as they do on the server.

- **One sender.** `commands.firmware_install` is called from one place,
  `update.py`'s `async_install`, which Home Assistant calls for the `update.install` action
  and nothing else. Setup, a state message, a reconnect, a reload and a newly staged image
  read the state and send nothing.
  `tests/test_update.py::test_firmware_nothing_installs_until_the_explicit_install_action`
  is the integration's mirror of the server's
  `nothing_installs_until_the_explicit_install_action`: set up with an update available,
  reload, lose and regain the stream, change the state, and the fake server has received no
  command at all.
  `test_firmware_install_action_sends_exactly_one_firmware_install` then holds the action to
  one command whose bytes are `fixtures/control/v2/firmware_install.json`.
- **What is offered.** `State.firmware_offer`: an image of `firmware.images` whose `verdict`
  is `verified`, for the speaker's `board`, with a version above the one it runs, and only
  while the speaker's `update_available` is true. A refused image, and one with no verdict or
  one this client does not know, is never the latest version. The server compares versions
  as names and would also install an older image; the integration does not offer one (digits
  compare as numbers, so 2.10.0 is above 2.9.0; of several the highest, then the first name).
- **What is shown.** `installed_version` is `speakers[].firmware.version`; `in_progress` is a
  `state` from `requested` to `pending_verify`; `update_percentage` is `received` of `size`
  while `requested` or `receiving` and nothing after; the attributes `install_state`,
  `reason`, `image`, `image_version` and `board` carry the outcome (`confirmed`,
  `rolled_back`, `refused`, `interrupted`, `cancelled`), which stays until the next install.
- **No entity without a report.** `speakers[].firmware` is written only once a speaker has
  reported; until then the speaker has its device and no update entity. If the member goes
  away the entity is unavailable.
- **The bench guard is the server's.** Nothing here sets `CHORUS_OWNER_AT_BENCH`. A refusal
  `owner-not-at-bench` is a translated error that names the variable; the tests' fake server
  refuses a speaker it is told is not on its host, and no test installs on anything.

## Speaker diagnostics: read from `/metrics`, rate-limited, mostly disabled

The model is `docs/decisions/0157-the-home-assistant-speaker-diagnostics.md`; the server's
side is `docs/telemetry.md`. The state message carries no telemetry (a report a second per
speaker never reaches a control subscriber), so the sensors read the exporter's text.

Every adopted speaker has these eight sensors on its device, all with the diagnostic entity
category:

| Sensor | Series of `GET /metrics` | Unit, state class | Enabled by default |
|---|---|---|---|
| Sync error | `chorus_speaker_sync_error_seconds` | microseconds, measurement | no |
| Buffer fill | `chorus_speaker_buffer_fill_seconds` | milliseconds, measurement | no |
| Rate correction | `chorus_speaker_rate_correction_ratio` | ppm, measurement | no |
| Resyncs | `chorus_speaker_resyncs_total` | a count, total increasing | no |
| Link | `chorus_speaker_link_info{link}` | `wired`, `wifi` or `unknown` | **yes** |
| Signal strength | `chorus_speaker_rssi_dbm` | dBm, measurement | no |
| Temperature | `chorus_speaker_temperature_celsius` | °C, measurement | no |
| Firmware version | `chorus_speaker_firmware_info{version}` | text | **yes** |

- **The poll interval is 60 s** (`METRICS_SCAN_INTERVAL`), one scrape for all speakers, by a
  second coordinator (`ChorusMetricsCoordinator`) that the state's coordinator does not
  depend on. With no diagnostic sensor enabled there is no scrape at all. The first enabled
  sensor added starts one scrape at once; a refresh asked for less than 55 s after the last
  scrape (`METRICS_MIN_GAP_SECONDS`, on a monotonic clock) is answered from that scrape, so
  within one loading of the entry no two scrapes are less than 55 s apart.
  `tests/test_sensor.py::test_sensor_scrapes_never_exceed_the_documented_rate` counts the fake
  server's `/metrics` requests over ten simulated minutes while something asks for a refresh
  every 5 s, and `test_sensor_no_scrape_while_no_diagnostic_entity_is_enabled` holds the
  count at zero.
- **A failed or malformed scrape** makes the diagnostic sensors unavailable, is logged once,
  and touches nothing else: the media players, the event stream and the entry stay as they
  are (`test_sensor_failed_or_malformed_scrape_leaves_the_media_players_alone`).
- **Unknown is not zero.** A series the exporter omits for a connected speaker (the signal
  strength of a wired speaker, a temperature nobody measures) is the state `unknown`. A
  speaker whose session ended keeps only its firmware version; its other sensors are
  unavailable.
- **The sync error is not timing evidence.** It is the speaker's own estimate of its playout
  error, never a measured error between speakers (`docs/telemetry.md`, "What is ASSUMED").
- **What the exporter does not carry, as follow-ups, not invented here.** K83 says
  "corrections": the exporter has the rate correction in force (a gauge) and no count of
  corrections, so the sensor is the gauge; a count would be a new server series. K83 says
  "temperature": the series exists, and no board profile has a temperature sensor, so no real
  speaker fills it and the sensor reads unknown until one does (`docs/telemetry.md` names
  wiring one as a follow-up). The exporter's underruns and heap figures are not in K83's list
  and are not sensors. A scrape of the real server through the integration is not in
  `tests/test_live_server.py` yet.
- The tests' sample (`tests/fixtures/metrics-scrape.txt`) is in the exporter's format, held
  to the families, HELP and TYPE lines of `crates/server/src/metrics.rs` by
  `tests/aiochorus/test_metrics.py`. It is not a capture of a running server.

## Speaker buttons: one event entity per button, one event per accepted press

The model is `docs/decisions/0161-the-home-assistant-speaker-button-events.md`. K83 asks for
button presses as event entities; K65 is the controller role they come from. The
source is the server's `GET /api/controller-events` (`docs/control-plane.md`, "How the
messages travel"; `docs/decisions/0151-controller-events-on-the-http-control-plane.md`): one
`controller_event` message per controller command the server accepted, sent once to whoever
is subscribed and never kept.

- **Which speakers.** One whose `speakers[].roles` contains `controller` gets five `event`
  entities on its device (`event.py`), device class `button`: play/pause, volume up, volume
  down, next and previous, the buttons of the compact speaker and the streaming amp
  (`firmware/include/chorus/controls.h`). `roles` are the speaker's latest hello's and empty
  while it is away, so the entities are added the first time the role is seen and stay until
  the speaker is forgotten (its device and entities are removed then).
- **Which entity, which event type.** The message carries the command, not the button, and
  `ControllerEvent.button` maps it by what the firmware's buttons send
  (`firmware/src/controls.c`):

  | `command` (and `value`) | entity | event type |
  |---|---|---|
  | `toggle` | Play/pause button | `press` |
  | `join`, `leave` | Play/pause button | `long_press` |
  | `volume_step`, above zero | Volume up button | `press` |
  | `volume_step`, below zero | Volume down button | `press` |
  | `next` | Next button | `press` |
  | `previous` | Previous button | `press` |

  The event's attributes are the message's other members: `command`, `value`, `target`,
  `room` (its `zone`) and `outcome`. The speaker is the message's `endpoint`, which is the
  adopted speaker's id.
- **Exactly one event per press.** The coordinator hands a message to the one entity
  registered for its (speaker, button) and to nothing else
  (`test_button_one_press_fires_exactly_one_event`, sent as the shared vector's bytes;
  `test_button_two_speakers_do_not_cross`).
- **A reconnect never replays or invents a press.** The server's stream opens with a comment
  line and no message, the reader (`ControllerEventStream`) keeps nothing between
  connections, and the entity fires only from a message. Losing the stream makes the button
  entities unavailable, since a press would be missed; coming back restores the last event's
  time and fires nothing (`test_button_stream_drop_and_reconnect_fires_nothing`: the press
  stream alone, the whole server, then a reload, with a press made while detached that is
  never delivered).
- **What is ignored.** A command no speaker button sends (`play`, `pause`, `volume_set`,
  `mute_set`, a later server's name), and an endpoint with no enabled button entity (a wall
  remote that is not an adopted speaker, a speaker that never declared the role), are dropped
  with one debug log line each and change nothing
  (`test_button_unknown_button_or_speaker_is_ignored_with_one_log_line`).
- **The reader.** `_aiochorus/client.py` reads both streams with the same code: the line
  splitter and its 8 MiB bound on one event (`sse.py`), a liveness check with `GET /api/state`
  after 45 s of silence, and the backoff from one second to one minute with jitter. A stream
  of presses is silent most of the time, so its backoff starts over when the stream opens,
  not at its first message. A message that is not a `controller_event` ends the stream, as a
  message that is not a state does. `tests/aiochorus/test_controller_events.py` reads the
  shared vectors `fixtures/control/v2/controller_event.json` and
  `controller_event-transport.json` (and their `.fields`), never a copy.
- **A server without the route** (older than the route) answers 404: the button entities stay
  unavailable, one log line says so, and everything else works
  (`test_button_server_without_the_route_leaves_the_rest_working`).
- **Nothing is sent.** The platform sends no command; what a button does in chorus is the
  server's. Device triggers are not provided: an automation triggers on the entity's state
  (the README has one).

## Voice rooms: an Assist satellite per room that has a microphone

The model is `docs/decisions/0176-the-home-assistant-voice-satellite.md` and, for what goes
beyond the basic run, `docs/decisions/0179-a-rooms-choice-of-wake-words.md`; the path is proposal
P8, Option A, and the server's half is `docs/control-plane.md`, "Voice: `voice_enabled` and
`mic_muted`" and "Voice: the wake word, the run and its audio".

- **Which rooms.** A room has a microphone while a speaker adopted into it lists `voice`
  among its `roles` (`State.voice_rooms`), present or not. Such a room gets three entities on
  its own device, and a room without one gets none: the satellite
  (`<server>:room:<room>:assist_satellite`), the Voice enabled `switch`
  (`...:voice_enabled`, configuration) and the Mic muted `binary_sensor` (`...:mic_muted`,
  diagnostic). They are added when a room gains a microphone and removed, from the registry
  too, when it loses its last one (`entity.py`, `async_setup_voice_room_entities`).
- **The two halves of the gate.** Voice enabled is the room's `voice_enabled`, off by
  default and kept by the server; the switch sends `voice_enabled` and shows what the state
  says. Mic muted is the room's `mic_muted`, which is the speaker's hardware switch as the
  speaker reports it: read-only, with no action, because no command opens a microphone.
- **Wake to stream to reply.** The first satellite opens the entry's stream of wake words
  (`GET /api/voice-events`); a server with no voice room is never asked for it. On a
  `voice_wake` for its room the satellite checks the room's gate as the last state said it,
  sends `voice_start`, and is answered with a `voice_run`: the run's identifier and its
  limit. It opens `GET /api/voice-audio?run=<identifier>`, checks the answer's
  `X-Chorus-Audio-Format`, and calls `async_accept_pipeline_from_satellite` with that
  stream, `start_stage` speech-to-text and the wake word's phrase, which is the path Home
  Assistant has for a wake word detected before the pipeline. The stream hands on whole
  16-bit samples as they arrive and ends when the server closes it; it also stops on its
  own two seconds past the run's limit, on a monotonic clock, and after ten seconds of
  silence on the socket.
- **Ending the run.** `voice_stop` is sent once per run: at the pipeline's `stt-end`, at its
  `error`, at its `run-end`, when the pipeline raises, when an announcement cancels it and
  when the entity is removed, whichever comes first. The microphone is therefore closed
  while Home Assistant thinks and speaks, not at the run's limit.
- **The reply and announcements.** The pipeline's `tts-end` carries a media URL of Home
  Assistant's; it and `assist_satellite.announce` go through the same code as the media
  player's announce (`announce.py`: resolved inside Home Assistant, refused unless it is
  Home Assistant's own origin, sent as `announce` for the room). The server's answer carries
  the announcement's number, and the entity waits until the pushed state lists that number
  as no longer `playing` ("How it ended"), for at most eleven minutes. `async_announce`
  returns then, and a reply calls `tts_response_finished` then. `failed` is a translated
  error; `displaced` (an alarm, a later announcement) is an end like any other.
- **While the gate is closed** no `voice_start` is sent and no pipeline starts; the server
  would refuse one by name (`voice-disabled`, `mic-muted`) and that refusal starts nothing
  either. Announcements are not gated: they use the speaker.
- **A server that serves no run here.** `voice_start` refused with `no-voice-integration`,
  or the audio route refused with `not-the-voice-integration`, is a repair issue naming the
  flag (`--voice-integration`); it is removed when a run's audio is next read.
- **Continue conversation.** The satellite reads `continue_conversation` off the pipeline's
  `intent-end`. When it is true and the reply was played to its end, it opens a second run
  itself (`voice_start` with no `voice_wake` before it) and starts a pipeline at
  speech-to-text with no wake word phrase; Home Assistant's base class carries the
  conversation's id over. Nothing is continued when the reply failed, when the gate closed,
  or when another run or a prompt came first.
- **Start conversation and ask question.** The entity declares `START_CONVERSATION` and
  implements `async_start_conversation`, which the base class calls for both actions: it
  plays the chime and the prompt as an announcement, waits for a run that was in progress to
  finish ending, opens a run and its audio, and returns; the pipeline then runs in the
  background (for a question the base class ends it at speech-to-text and returns the
  words). A wake word during the prompt is ignored. Where the room does not listen, the
  prompt is played and the call raises `not_listening_voice_disabled`,
  `not_listening_mic_muted` or `not_listening` (with the server's reason), from the last
  state's gate without asking the server, or from the server's own refusal by name. A
  question whose pipeline is over without an answer (it raised before its first event) is
  answered with none, so the action cannot wait for ever.
- **Timers.** The satellite registers Home Assistant's timer handler
  (`intent.async_register_timer_handler`) for the room's device, which is what lets a timer
  be set by voice there. A `finished` event plays Home Assistant's own chime three times
  through `announce` (`TIMER_SOUND`, `TIMER_SOUND_TIMES`), each waited for; every other
  event is a debug line. The manifest lists `intent` for it.
- **The wake word choice.** `async_get_configuration` lists the state's `wake_words` as
  available and, as active, the room's own `wake_words` (every one for a room that never
  chose); `async_set_configuration` sends `voice_wake_words` with the chosen ids, and an id
  the server does not run is the translated `unknown_wake_word`
  (`docs/decisions/0179-a-rooms-choice-of-wake-words.md`).

Privacy (I4): the integration holds no microphone audio beyond the chunk in flight, writes
none to a log or a file, and never logs the run's identifier (`VoiceRun` keeps it out of its
`repr`, and the client's errors for the route never quote the request).
`tests/test_assist_satellite.py` holds all of the above against the fake server and a faked
pipeline, and `tests/aiochorus/test_voice.py` holds the client to the shared vectors
(`voice_enabled.json`, `voice_start.json`, `voice_stop.json`, `voice_wake.json`,
`voice_run.json`, `state-voice.json`, `error-voice-start-*.json`, `voice_wake_words.json`,
`state-wake-words.json`, `error-voice-wake-words-unknown.json`).

## The room visualizer: one sensor per room, disabled by default, five states a second at most

The model is `docs/decisions/0162-the-home-assistant-visualizer-entity.md`. Brief section 23
item 1 asks for "the visualizer stream exposed so HA automations can map it to lights"; the
source is the server's `GET /api/visualizer?zone=<room>` (`docs/visualizer.md`, "The HTTP
stream"; `docs/decisions/0153-the-visualizer-stream-over-http.md`): one `visualizer` message
per frame, at most ten a second, the latest only.

- **The entity.** One `sensor` per room on the room's device (`visualizer.py`, added by
  `sensor.py`), unique id `<server>:room:<room>:visualizer`, no entity category, and
  `entity_registry_enabled_default` false. Its state is the frame's `peak` as a percentage of
  the level byte; its attributes are `rgb_color` (`red`, `green`, `blue`), `brightness`,
  `transition` (`transition_ms` in seconds), `beat` and `lead_ms`. The first three are the
  names and units `light.turn_on` takes, so an automation passes them through
  (`test_visualizer_frame_becomes_a_state_with_a_colour_for_a_light`, sent as the shared
  vector `fixtures/visualizer/http-frame.json`).
- **No stream while disabled.** The stream is opened in the entity's `async_added_to_hass`
  and closed when the entity is removed; Home Assistant never adds a disabled entity. So a
  room whose visualizer is disabled has no subscriber on the server, which then does not
  analyse the room's slot for it (`docs/visualizer.md`, "Where it is computed")
  (`test_visualizer_disabled_holds_no_stream_open`: no request to the route at all until one
  room is enabled, then that room's stream alone, closed again on disable and on unload).
- **The rate cap is the entity's own.** At most one state write every
  `VISUALIZER_MIN_WRITE_INTERVAL` (200 ms: five a second), on the event loop's monotonic
  clock, whatever arrives. The rule is the server's ("The drop rule"): the latest frame
  supersedes the ones before it and nothing is queued; a write that the cap holds back
  happens when its interval runs out, not at the next frame; and a beat no write has shown
  rides in the next one. Every write of the entity goes through the cap, a change of
  availability and a change of the house's state included, and a write that would say what
  the last one said is not made.
  `test_visualizer_state_writes_never_exceed_the_cap_whatever_the_frame_rate` counts the
  calls of `async_write_ha_state` over simulated time against a fake server with no cap of
  its own, at 100 frames a second, in bursts of 20 at one instant, and at the server's ten a
  second: no two writes are closer than the interval.
  `test_visualizer_beat_held_back_by_the_cap_rides_in_the_next_write` holds the beat rule.
  200 ms is ASSUMED, not measured: half the server's rate, and no report in
  `docs/measurements/` says what a Home Assistant host or a lamp takes.
- **Idle.** State `0`, `rgb_color` `(0, 0, 0)`, `brightness` 0, `beat` 0, `transition` 0,
  `lead_ms` 0: before the first frame, at a silent frame (`peak` 0 and `beat` 0, which is
  the first of a run of silence and the last thing the server sends, still carrying the
  colour in force), and `VISUALIZER_IDLE_AFTER` (2 s, ASSUMED) after the last frame when no
  silent one came, which is what a room whose group was given no source looks like
  (`test_visualizer_silence_leaves_the_idle_value`).
- **The recorder.** No state class, so the sensor platform's statistics compiler never
  takes it and it has no long-term statistics; `_unrecorded_attributes` is `MATCH_ALL`, so
  the recorder stores none of its attributes (`test_visualizer_keeps_out_of_long_term_history`
  holds both on the entity; it does not run a recorder). The state itself is recorded in the
  short-term history like any entity's, at most five rows a second while the room plays,
  until the purge: an integration has no way to exclude its own entity's states, and the
  README gives the owner the `recorder: exclude:` lines.
- **Unavailable while the stream is not attached**: lost, or a server older than the route
  (404). One info log line says so and one that it is back; the reader reconnects with the
  same backoff. Coming back is idle, never the last frame
  (`test_visualizer_stream_lost_is_unavailable_then_idle_and_logged_once`).
- **The reader** is the third use of `_aiochorus/client.py`'s one subscriber class
  (`VisualizerStream`): the same line splitter and bound, liveness check and backoff, which
  starts over when the stream opens because a quiet room's stream is empty. `VisualizerFrame`
  reads the members a lamp needs and not `timestamp_ns`, which is on a clock a subscriber
  does not have. A frame that names another room ends the stream.
  `tests/aiochorus/test_visualizer.py` reads the shared vector and its `.fields`, never a
  copy.
- **The example automation is tested as written.**
  `test_visualizer_documented_automation_calls_the_light_with_the_frames_colour` takes the
  YAML out of the README, sets it up in Home Assistant with the sensor's entity id put in,
  sends the shared frame and checks that `light.turn_on` was called with the frame's colour.
- **Timing.** A state is written when a frame arrives or when the cap lets it; `lead_ms` is
  passed on and not waited for. This is not a claim about a light: nothing has measured one.
- **Nothing is sent, and no light is driven.** The mapping is the owner's automation; the
  integration ships none and no blueprint.

## The dashboard: stock cards only

The model is `docs/decisions/0163-the-home-assistant-dashboard-is-stock-cards-only.md`; the
choice is the owner's (proposal P10, Option A, approved at Checkpoint K). The file is
`integrations/homeassistant/dashboard/chorus.yaml`: one Sections view, "Music", made of
Home Assistant's own cards and tile features. There is no custom card, no resource, no
script and no link in it, and the integration serves nothing to the frontend. It is an
example to copy, not something the integration installs.

### What it shows

| Section | Cards |
|---|---|
| Groups | A tile per saved group: on and off, the volume slider with mute, the source. The slider is the group volume, which scales every room of the group relatively (K77) |
| Now playing | A media control card per room, with the title, the artist and the artwork. A room's card shows only while its group has a now-playing record (state `playing`, `paused` or `buffering`); a room that plays the server's stream or a line-in has none |
| One per room | The room's tile (on and off, volume with mute, source; pause, next and previous while a Spotify receiver plays); **Group volume**, shown only while the room is grouped; Bass and Treble; Loudness, Night mode, Speech enhancement and Quiet hours |

The features used are `media-player-playback`, `media-player-volume-slider`,
`media-player-source`, `numeric-input` and `toggle`; the cards are `heading`, `tile` and
`media-control`.

**The group-volume tile** is a tile for the room's group-volume `number` with a visibility
condition `numeric_state`, `above: -1` on that number. The number is unavailable while the
room is alone, and `unavailable` is not a number, so the tile is hidden then and appears
when the room joins a group, live or saved. Its slider is the volume of the group the room
is in.

**Not on it:** the room's Input `select` (the tile's source control sends the same command),
the autoplay switches (their ids carry the input's id; they are on the room's device page),
the speakers' firmware, diagnostics and buttons, and the visualizer.

### Grouping

Grouping from Home Assistant is its own join dialog: open a room's tile, press the join
button, tick the rooms. The dialog lists the chorus rooms and nothing else: not the saved
groups, which cannot be joined, and not another brand's players. Join is "take the room"
and unjoin is `take` on the room, as above. Which rooms play together shows as the group
volume tile appearing and in the dialog's ticks; Home Assistant has no map of groups.

Drag-to-group is the chorus app's, not Home Assistant's: no stock card does it, and a
custom card was declined (P10, Options B and C). Home Assistant 2026.9, and the 2026.10
beta read on 2026-10-04, have no group volume and no grouping control of their own
(0163, "What was read").

### Adapting it to a house

The entity ids in the file are the ones Home Assistant 2026.9 gives the rooms of the tests'
house: a living room, a kitchen, a study, a bedroom, and one saved group, Downstairs. A
default id is the area, then the device, then the entity, and the area and the device are
both the room's name, so the name appears twice: `media_player.living_room_living_room`,
`number.living_room_living_room_group_volume`, `switch.kitchen_kitchen_night_mode`. A saved
group has no area: `media_player.downstairs`.

1. Find your ids on each room's device page (Settings, Devices and services, chorus). If
   you renamed an entity or assigned the room to another area before the entity was made,
   the id is what the page says, not the pattern.
2. In a copy of the file, replace each room's prefix (`living_room_living_room`) by yours,
   and the `heading`, `name` and `icon` of its section. A room's section is one block from
   `- type: grid` to the next; copy it for a room more, delete it for a room less, and do
   the same with the room's card in "Now playing".
3. In "Groups", list a tile per saved group.
4. To offer only some sources on a tile, give `media-player-source` a `sources:` list of
   the labels in the player's `source_list`.
5. Register the copy as a YAML dashboard (`lovelace: dashboards:` with `mode: yaml`), or
   paste the view into a dashboard's raw configuration editor. An installation that hides
   cards for rooms that do not exist yet adds a visibility condition to each section, naming
   the states its room's media player has (`"off"`, `"on"`, `idle`, `playing`, `paused`,
   `buffering`, the first two quoted: bare, YAML reads them as booleans).

Home Assistant shows a warning in place of a card whose entity does not exist, except the
conditional ones, which
stay hidden; so a wrong id is seen at once.

### The test

`tests/test_dashboard.py` (`make ha-test HA_TEST_ARGS="-k dashboard"`) holds the file
without a browser:

- it parses with Home Assistant's YAML loader into one Sections view;
- every entity it names, in a card or in a visibility condition, is an enabled entity of
  the integration with a state, after setup against the fake server; every media player of
  that house and every room's group volume is on it; each feature sits on the kind of
  entity it is for;
- every view, section, card, badge and feature `type` and every visibility `condition` is
  on an explicit list of stock types, and no mapping with a `type` escapes the walk;
- no `resources`, no action, no URL and no `custom:` anywhere, comments included;
- the group-volume tiles' conditions hold while the rooms are grouped and fail for a room
  that left its group;
- each check names a bad example made for it (a custom card, a missing entity, a link).

It does not render the dashboard: how it looks was not seen here, and that the frontend
treats `unavailable` as failing a numeric test is ASSUMED from the documentation.

## Installing: the pinned copy

The decision is `docs/decisions/0164-the-home-assistant-integration-is-installed-from-a-pinned-export.md`.

**HACS is not used.** HACS installs from a public repository's releases and updates on its
own schedule; the integration is installed instead as a copy of one chorus commit that the
installation keeps in its own repository, reviews like any change, and mounts read-only. Home
Assistant can then load the integration and cannot change it, and what runs is always a
commit that passed the gate here.

**The pinned copy** is what `tools/ha-export.sh` writes:

```sh
tools/ha-export.sh <dir> [<commit>]     # <dir>/chorus and <dir>/chorus.lock; default HEAD
tools/ha-export.sh --verify <dir>       # the copy is the commit its lock names, byte for byte
```

- `<dir>/chorus` is `integrations/homeassistant/custom_components/chorus` as the commit has
  it: the directory Home Assistant loads as `custom_components/chorus`. It is read from git's
  objects, never from the working tree, so an uncommitted edit, an untracked file or byte
  code cannot reach it.
- `<dir>/chorus.lock` is the manifest: the commit (the pin), the integration's version, the
  Home Assistant version of the harness pin at that commit, the file count, and one
  `sha256sum` line per file. An installation without a chorus checkout checks its copy with
  `sha256sum --check --strict chorus.lock`; with one, `--verify` also exports the commit
  again and compares, which catches a file and its hash line edited together.
- Two exports of one commit are the same bytes: no date, host or user is written, files are
  `0644` and directories `0755`. `tools/conventions/check-ha-export.sh` holds that, and
  every way `--verify` must refuse a copy, in the gate.

**The install**, where the copy is vendored (the owner's homelab repo does it this way):

1. The copy and its lock are committed beside the Home Assistant configuration, as
   `custom_components/chorus` and `custom_components/chorus.lock`, and never edited there.
   That repository's own check holds the copy to the lock.
2. The Home Assistant container binds that one directory read-only at
   `/config/custom_components/chorus`. `/config/custom_components` itself stays as it is.
3. Home Assistant is restarted (it loads a custom integration at start only).
4. In Home Assistant: **Settings > Devices & services > Add integration > chorus**, with the
   server's host and control port (`integrations/homeassistant/README.md`, "Installation
   parameters"). The form refuses a server older than the integration needs and says so: the
   server must be a build of the pinned commit or a later one.
5. The dashboard (`integrations/homeassistant/dashboard/chorus.yaml`) is copied, adapted to
   the house ("Adapting it to a house" above) and registered as a YAML dashboard. It needs
   no resource.

Steps 3 and 4, and the merge and deploy before them, are the owner's actions; nothing in
this repository performs them.

### Updating: how a new pin is made

A pin is a commit on `main` whose gate passed (`ha-test`, `ha-hassfest`, `ha-live`); never a
branch head, and no release tag is needed.

1. In a chorus checkout that has the commit:
   `tools/ha-export.sh <the installation's custom_components directory> <commit>`. The export
   replaces the earlier copy whole, so a file the new commit dropped goes too.
2. `tools/ha-export.sh --verify <that directory>`, then that repository's own checks.
3. One change there holding the copy and the lock, reviewed and merged by the owner;
   Home Assistant restarts when it is deployed, which is what loads the new code.
4. A decision record here names the new pin when it changes what the installation can do
   (a new platform, a server version it needs); a pin that only follows fixes needs none.

**When the pin must move:** when the installation's Home Assistant version moves (the harness
pin moves with it, "Moving the pin" below, and the copy that was tested under the new version
follows); when the server moves to a control catalog the pinned integration does not speak;
and when a fix or a feature of the integration is wanted. Between pins nothing updates by
itself.

**Removing it:** delete the entry in Home Assistant, then the bind, the directory and the
lock, and restart.

## The upstream draft: what changes for Home Assistant core

The integration is written as a draft for Home Assistant core (K42, K61): it is installed as
a custom integration and kept in the form core asks for. **Submitting it is the owner's
choice and the owner's action**; no task of this repository opens a pull request, an issue
or a comment outside the owner's repositories. Asked on 2026-10-06, the owner chose **not yet** (decision
0227): nothing is submitted until the owner tells a chorus session to. The draft is this directory at a commit on
`main`: `integrations/homeassistant/custom_components/chorus` with its tests in
`integrations/homeassistant/tests` and its documentation in
`integrations/homeassistant/README.md`.

**What changes between the custom form and the core form** is exactly what
`tools/ha-hassfest.sh` already does to the copy it grades as core, in every gate run:

| Change | Why |
|---|---|
| The directory becomes `homeassistant/components/chorus` | where core's integrations live |
| `version` leaves `manifest.json` | core's manifest schema has no such key; a core integration is versioned by the release |
| `issue_tracker` leaves `manifest.json` | core's schema has none; issues go to core's own tracker |
| `documentation` becomes `https://www.home-assistant.io/integrations/chorus` | the core form of the URL |
| `homeassistant.components.chorus.*` joins `.strict-typing` | the `strict-typing` rule; the code already passes `mypy --strict` under core's own mypy version |
| `brand/` is left out | core's brands live in another repository (`home-assistant/brands`); the icon and logo are submitted there |

With those changes and nothing else, hassfest at the pinned core tag passes the integration
as a core integration and grades `quality_scale.yaml` (the run proves it graded the file by
failing the same copy with one rule removed). `make ha-hassfest` prints the three runs.

**What a submission needs beyond that**, none of which `ha-hassfest.sh` does. These come from
Home Assistant's developer documentation as read for decision 0138 and are to be read again
when the owner submits, since core's rules move:

- **The client library on PyPI.** Core takes no vendored protocol code: `_aiochorus` would be
  published as a package of its own, built by public CI, and named in `requirements`
  (`quality_scale.yaml` exempts `dependency-transparency` for exactly this, with its reason).
  It is the one change that needs a public repository, and it reverses "no runtime
  requirement" above for the core form only; the custom form keeps the vendored client.
- **The tests move** to `tests/components/chorus` and import
  `homeassistant.components.chorus`; core's own fixtures replace the harness, which exists to
  give a custom integration those fixtures. The fake server and the shared vectors
  (`fixtures/control/v2`) the tests read from this repository would be copied in.
- **The documentation** (`README.md`, which carries the heading each `docs-*` rule names)
  becomes a page in Home Assistant's documentation repository.
- **Generated files** (`CODEOWNERS`, the generated config-flow and zeroconf lists, the
  requirements lists, `mypy.ini`) are regenerated by core's own scripts in the core checkout.
- **The tier** in the manifest (`quality_scale: platinum`) is a self-certification here; in
  core it is the reviewers' to grant.

The owner's item for it is in the owner's queue, with this section as what it carries.

## The security rules

Home Assistant is reachable from more places than chorus is, and anyone logged into it can
operate every entity. The control plane has no authentication. So:

1. **No unauthenticated endpoint** (brief section 4.8). Every HTTP view requires auth and
   every webhook is local-only; the integration serves no static path and injects no script.
   Today it registers none of them at all. `tests/test_no_unauthenticated_endpoint.py`
   holds it three ways: at run time (the aiohttp router and the webhook registry compared
   before and after the integration and all its platforms are set up), statically (`ast` over
   every file), and against itself (a bad view, a bad webhook and a static path must each be
   named). `tools/conventions/check-ha-integration.sh` is the grep-level backstop. The
   voice satellite adds none: it depends on Home Assistant's `assist_satellite` and
   `assist_pipeline`, whose own views are Home Assistant's and are set up before the test
   takes its first snapshot, and the microphone audio is pulled from the chorus server, never
   posted to Home Assistant.
2. **No arbitrary fetch.** An announcement's URL is resolved inside Home Assistant and must
   have the origin of Home Assistant's own internal or external URL, or the integration
   refuses before any request is sent. The server holds its own list (`--announce-origin`)
   and refuses too. `play_media` without `announce` takes `chorus://input/...` only.
3. **The server's clamps win.** The integration never caches a volume it asked for.
4. **Refusals are mapped by `field`**, never by the wording of `detail`: `url`, `target`,
   `source`, `group`, `zone`, `volume`, `t` (the server cannot serve the command now: no
   player, or none free), `speaker`, `image`, and a general one for any other. The one
   exception is a refusal the catalog itself names: `firmware_install`'s start their `detail`
   with a name and a colon, and `owner-not-at-bench`, `busy` and `image-not-verified` each
   have their own message, picked by that name and never by the words after it. When an announcement's `url` is
   refused, the integration asks `GET /api/server` again and, if this Home Assistant's
   origin is not among `announce_origins`, says exactly that and names the flag.
5. **No runtime pip**, as above.
6. **Diagnostics are an allowlist**: named fields of the state, never the state itself. No
   host, URL, key fingerprint, speaker or endpoint id, stored source value or track title.
7. **Bounded use of Home Assistant.** One subscriber per entry (and one per enabled
   visualizer sensor, whose state writes are capped at five a second; one more for wake
   words once a room has a microphone; one connection per open voice run, bounded by the
   run's limit), reconnect with backoff
   and jitter, one event bounded at 8 MiB, commands one at a time per platform.

## The test harness and its pin

Tests run under `pytest-homeassistant-custom-component`, which brings Home Assistant's own
test fixtures for exactly one Home Assistant version. The pin is recorded once, in
`integrations/homeassistant/harness.pin`, and `check-ha-integration.sh` holds `pyproject.toml`,
`uv.lock` and `.python-version` to it.

**The rule: the pin moves when the homelab's Home Assistant pin moves, and only then.**

The tests' fake server serves the repository's shared vectors (`fixtures/control/v2/state-*.json`,
read from the repository, not copied) and the command bytes are compared with the shared
command vectors (the server's identity, `announce` and its refusals among them), so the
Python client and the Rust server cannot drift apart. The integration's tests keep no copy of
any message.

The voice satellite's platform loads Home Assistant's Assist components, which import
packages the harness does not bring (`hassil`, `home-assistant-intents`,
`gazetteer-matcher`, `pymicro-vad`, `pyspeex-noise`, `mutagen`, `ha-ffmpeg`). They are in the
`dev` group at the versions core 2026.9.3's own manifests pin and locked by hash like the
rest, and they move with the pin. They are test-only: on a running Home Assistant they are
core's requirements, and the integration's manifest still lists none. `tests/conftest.py`
sets up Home Assistant's own `homeassistant` integration (which `conversation` reads) and
keeps `tts` and `ffmpeg` from touching the host at setup.

`tests/test_live_server.py` drives the real `chorus-server` on loopback when
`CHORUS_SERVER_BIN` names one (`CHORUS_SERVER_BIN=<path> make ha-live`, or `make ha-test`),
and is skipped by name otherwise. Its voice part switches the real server's `voice_enabled`
through the integration's switch and holds each refusal of a run and of the audio route by
its name; no speaker with a microphone can be attached to the server from Python, so a whole
run is `tests/test_assist_satellite.py`'s, against the fake.

```sh
make ha-test        # uv sync --locked, ruff, mypy --strict, pytest with coverage
make ha-hassfest    # hassfest as core (in a throwaway worktree), its proof, and as custom
make ha-test HA_TEST_ARGS="-k announce"                                    # a narrowed run
make ha-test HA_TEST_ARGS="tests/test_no_unauthenticated_endpoint.py"      # one file
```

**The whole run and a narrowed run.** `make ha-test` with no arguments is the gate step: ruff,
`mypy --strict`, every test, coverage held above 95 % overall and at 100 % for the config flow,
and a last line `ha-test: PASS`. With `HA_TEST_ARGS` it is the inner loop: pytest alone with
those arguments, passing or failing on the selected tests, with no ruff, no mypy and no coverage
threshold (pass `--cov` to see a report), and a last line `ha-test: NARROWED PASS`, which the
gate does not take for a pass.

**In the gate these steps run or they are red.** A missing `uv`, a core checkout that cannot be
cloned, or (under `CI=true`) an unset `CHORUS_SERVER_BIN` for `make ha-live` fails the step
naming what is missing; none of them prints a green `SKIPPED`. The gate also holds each of
`ha-test`, `ha-hassfest` and `ha-live` to its own `<step>: PASS` line, so a step that exits 0
without having run is red too
([0142](decisions/0142-the-home-assistant-gate-steps-run-or-fail.md)). Outside the gate and
CI, `make ha-live` without `CHORUS_SERVER_BIN` still skips the test by name and ends with
`ha-live: SKIPPED`.

The virtual environment (`UV_PROJECT_ENVIRONMENT`, default `/cache/venvs/chorus-ha`) and the
core checkout (`CHORUS_HA_CORE`, default `/cache/chorus-ha-core`) live outside the repository.
The first run fetches the pinned Python, the locked packages and the core tag; later runs need
no network.

### Moving the pin

1. Find the harness release whose `requires_dist` is `homeassistant==<the homelab's new
   version>` (`https://pypi.org/pypi/pytest-homeassistant-custom-component/<version>/json`),
   and its wheel's sha256.
2. Edit `harness.pin` (`homeassistant`, `homelab-commit`, `harness-version`,
   `harness-wheel-sha256`, `core-tag`, `core-commit` from `git ls-remote`, and `python` if the
   new core needs a newer one) and the same versions in `pyproject.toml`. Take `mypy` and
   `ruff` from the new tag's `requirements_test.txt` and `requirements_test_pre_commit.txt`,
   and the `hassfest` group from its `requirements_all.txt`. Take the Assist packages of the
   `dev` group from the `requirements` of the new tag's `assist_satellite`, `assist_pipeline`,
   `conversation`, `tts` and `ffmpeg` manifests.
3. `uv lock` in `integrations/homeassistant`, then regenerate `quality-scale-rules.txt` from
   the new tag's `script/hassfest/quality_scale.py` and reconcile `quality_scale.yaml`.
4. `make ha-test`, `make ha-hassfest`, `make gate-fast`. One commit, saying why.
