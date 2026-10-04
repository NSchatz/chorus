# chorus and Home Assistant

The integration lives in `integrations/homeassistant/custom_components/chorus` (goal 18). This
document is how it maps chorus to Home Assistant, the security rules it is held to, and its
test harness. What a person installing it reads is `integrations/homeassistant/README.md`; why
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
| A room (zone) | One device named for the room, `suggested_area` its name, with a `media_player`, a group-volume `number`, and its sound controls: bass and treble (`number`), loudness, night mode and speech enhancement (`switch`), an input `select`, a quiet-hours `switch`, and an autoplay `switch` for each autoplay rule that targets it |
| A saved group | One device under the server with a `media_player`, always present, and an autoplay `switch` for each autoplay rule that targets it |
| A live group | No entity: the `group_members` of its rooms' players, leader first |
| An adopted speaker | One device named for the speaker, `via` its room's device while it has a room and the server otherwise, a firmware `update` entity once it has reported what it runs, and eight diagnostic `sensor`s |

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

The model is `docs/decisions/0000-the-home-assistant-speaker-diagnostics.md`; the server's
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

## The security rules

Home Assistant is reachable from more places than chorus is, and anyone logged into it can
operate every entity. The control plane has no authentication. So:

1. **No unauthenticated endpoint** (brief section 4.8). Every HTTP view requires auth and
   every webhook is local-only; the integration serves no static path and injects no script.
   Today it registers none of them at all. `tests/test_no_unauthenticated_endpoint.py`
   holds it three ways: at run time (the aiohttp router and the webhook registry compared
   before and after the integration and all its platforms are set up), statically (`ast` over
   every file), and against itself (a bad view, a bad webhook and a static path must each be
   named). `tools/conventions/check-ha-integration.sh` is the grep-level backstop.
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
7. **Bounded use of Home Assistant.** One subscriber per entry, reconnect with backoff and
   jitter, one event bounded at 8 MiB, commands one at a time per platform.

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

`tests/test_live_server.py` drives the real `chorus-server` on loopback when
`CHORUS_SERVER_BIN` names one (`CHORUS_SERVER_BIN=<path> make ha-live`, or `make ha-test`),
and is skipped by name otherwise.

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
   and the `hassfest` group from its `requirements_all.txt`.
3. `uv lock` in `integrations/homeassistant`, then regenerate `quality-scale-rules.txt` from
   the new tag's `script/hassfest/quality_scale.py` and reconcile `quality_scale.yaml`.
4. `make ha-test`, `make ha-hassfest`, `make gate-fast`. One commit, saying why.
