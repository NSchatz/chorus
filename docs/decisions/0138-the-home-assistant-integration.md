# 0138: the Home Assistant integration is a custom integration written to all four quality-scale tiers, with a vendored client and no runtime requirement, rooms and saved groups as media players, join and unjoin as take the room, announcements from Home Assistant's own origin only, and no endpoint of its own

- Status: accepted (goal 18, 2026-10-04)
- Decided by: the owner for the shape (K59, K61, K64, K77, K78, I14; brief section 4.8) and
  the coordinator's goal-18 design for the tier, the pin, the path and the entity model;
  track `chorus-g18/ha-integration` for everything this record marks as its own choice
- Implemented in: `integrations/homeassistant/` (the integration, its vendored client, its
  tests), `tools/ha-test.sh`, `tools/ha-hassfest.sh`,
  `tools/conventions/check-ha-integration.sh`, `tools/gate.sh` (steps `ha-test`,
  `ha-hassfest`), `docs/home-assistant.md`, `docs/conventions.md` rule 25

## Context

Goal 18 puts chorus into Home Assistant: a media player per room and per saved group, with
join, unjoin, group volume, inputs and announcements, held to Home Assistant's own quality
bar and to brief section 4.8 (no unauthenticated endpoint). The server side (the server's
identity, the `announce` command, the TXT `id=`) is ADR 0136; this record is the Python side.

## What was read

All on 2026-10-04 unless a line says otherwise. No GPL source was opened; Home Assistant core
is Apache-2.0 and was read at tag 2026.9.3 (commit `6de5eb18cd4502f94af44cfff3a02250d88716ed`),
from the installed package and a shallow clone of
https://github.com/home-assistant/core/tree/2026.9.3.

- `script/hassfest/quality_scale.py` (`ALL_RULES`: 20 Bronze, 10 Silver, 21 Gold, 3 Platinum;
  `validate_iqs_file` returns at once for an integration that is not core),
  `script/hassfest/manifest.py` (the core and the custom manifest schemas: core has no
  `version` and no `issue_tracker` and wants a `www.home-assistant.io/integrations` URL),
  `script/hassfest/__main__.py` (`--integration-path`, the plugin list), `mypy.ini`
  (components may re-export what they import), `requirements_test.txt` (mypy 2.3.1),
  `requirements_test_pre_commit.txt` (ruff 0.16.3), `requirements_all.txt`
  (infrared-protocols 9.0.0), `.python-version` (3.14.5), `pyproject.toml`
  (`requires-python >= 3.14.2`).
- `homeassistant/config_entries.py` (`_abort_if_unique_id_configured(updates=)`,
  `_abort_if_unique_id_mismatch`, `async_update_reload_and_abort`, `_get_reconfigure_entry`),
  `helpers/service_info/zeroconf.py`, `helpers/update_coordinator.py`,
  `helpers/device_registry.py` (`DeviceInfo` carries `via_device_id`, a device id; the old
  `via_device` is deprecated; `async_get_device_by_identifier`), `helpers/entity_registry.py`
  (an entity id is built from the area, the device and the entity name), `helpers/network.py`
  (`get_url`), `helpers/http.py` (`HomeAssistantView.register`, `request_handler_factory`:
  the view is in the handler's closure), `components/http/server.py` (`register_view`,
  `async_register_static_paths`), `components/webhook/__init__.py` (`async_register`,
  `local_only`, the registry), `components/media_player/const.py`
  (`MediaPlayerEntityFeature`), `components/media_player/browse_media.py`
  (`async_process_play_media_url`, the paths it does not sign), `loader.py` (`has_branding`:
  a `brand` directory in the integration).
- `pytest_homeassistant_custom_component/plugins.py` 0.13.366 (sockets are blocked except
  127.0.0.1; the `socket_enabled` fixture).
- PyPI, each read 2026-10-04: https://pypi.org/pypi/pytest-homeassistant-custom-component/0.13.366/json
  (MIT; requires `homeassistant==2026.9.3`),
  https://pypi.org/pypi/homeassistant/2026.9.3/json (Apache-2.0),
  https://pypi.org/pypi/mypy/2.3.1/json (MIT), https://pypi.org/pypi/ruff/0.16.3/json (MIT),
  https://pypi.org/pypi/infrared-protocols/9.0.0/json (MIT), https://pypi.org/pypi/uv/json
  (MIT OR Apache-2.0).
- chorus's own: `docs/control-plane.md`, `fixtures/control/` and `fixtures/control/v2/`,
  the planning research `research-ha-integration.md` (sections 0, 1, 2, 7, 8, read
  2026-09-29 by its author) and `verify-ha-casting.md` (rows 5, 6, 7 and CORRECT item 2),
  `tools/control-plane-run.sh` and `crates/server/tests/common/mod.rs` (the server's command
  line, for the live test).

## Decision

**The tier.** The integration is written to every rule of Bronze, Silver, Gold and Platinum
as core 2026.9.3 lists them: 54 rules, 44 `done` and 10 `exempt`, in
`custom_components/chorus/quality_scale.yaml`. The exemptions and their reasons:
`reauthentication-flow` (the control API has no credentials); `dependency-transparency` (a
private repository cannot publish the client as an open PyPI package built by public CI; the
client is vendored); `action-setup` and `docs-actions` (it registers no action of its own);
`docs-conditions` and `docs-triggers` (it provides neither); `appropriate-polling` (it does
not poll); `docs-configuration-parameters` (no options flow); `entity-category` and
`entity-disabled-by-default` (every entity is a primary control). `brands` is `done` by a
`brand/` directory in the integration (an icon drawn by a script, chorus's own). The `docs-*`
rules are met by `integrations/homeassistant/README.md`, which carries the heading each rule
names. The manifest says `quality_scale: platinum`; a custom integration cannot hold a tier,
so that is a self-certification, and `make ha-hassfest` is what grades it: Home Assistant's
hassfest over a copy placed in a throwaway worktree of the pinned core checkout, with only
the changes being core demands (no `version`, no `issue_tracker`, the core documentation URL,
the domain in `.strict-typing`, no `brand/`), then the same copy with one rule removed, which
must fail, then the unpatched tree as a custom integration.

**The pin.** `pytest-homeassistant-custom-component==0.13.366` (requires
`homeassistant==2026.9.3`, the homelab's pinned version at homelab commit f0284f2), Python
3.14.8, recorded once in `integrations/homeassistant/harness.pin`. It moves when the homelab's
Home Assistant pin moves. mypy and ruff are the versions core 2026.9.3 itself pins, so
`mypy --strict` here and core's strict typing mean the same thing. Python 3.14.8 is this
track's choice: the newest 3.14 patch release, above core's floor of 3.14.2 (core's own
`.python-version` says 3.14.5; which patch the Home Assistant container runs is ASSUMED to be
at or below it and not to matter to a pure-Python integration). uv installs both and is
pinned in `mise.toml`: one tool gives a hash-locked environment (`uv.lock`, a sha256 per
file) and the exact interpreter, which is what the pin rule asks of anything that checks
chorus.

The direct development dependencies, none of which ships or is imported at run time: Home
Assistant 2026.9.3 (Apache-2.0, through the harness), pytest-homeassistant-custom-component
0.13.366 (MIT), mypy 2.3.1 (MIT), ruff 0.16.3 (MIT), infrared-protocols 9.0.0 (MIT; imported
by hassfest, at the version core's `requirements_all.txt` pins), and uv 0.12.22 (MIT OR
Apache-2.0). The harness's transitive packages (159 locked) are development tools; the
licence allowlist of conventions rule 13 ranges over what chorus builds and ships, and the
integration ships no dependency at all.

**The vendored client.** `custom_components/chorus/_aiochorus/` is the client's one source
location: async only, typed, no Home Assistant import, an injected session. It builds each
command's bytes by hand in the catalog's canonical encoding and is held to the shared
vectors. It reads the event stream in chunks and splits lines itself, with an 8 MiB bound on
one event (this track's number: far above any state a house produces, small enough to refuse
a peer that never ends a line), because a state message can exceed a reader's default line
limit. Reconnect is exponential from 1 s to 60 s with jitter of half to all of each step
(this track's numbers). A stream silent for 45 s is probed once with `GET /api/state`: the
control plane documents no heartbeat, so silence alone cannot tell an idle house from a dead
connection.

**The entity model** (research section 2.2, I14). One `media_player` per room; one per saved
group, always present, without GROUPING, its rooms as an attribute; live groups only as the
rooms' `group_members`, the leader being the group's first room in the server's order, a room
alone listing itself; a per-room group-volume `number`, available while the room is grouped.
One server device, one device per room with `suggested_area` (never an area created or
assigned), one per saved group under the server. State: `off` for the source `none`; the
now-playing record's state when there is one; `idle` for a player or receiver with no record;
`on` otherwise (the stream, a line-in), because the server does not say whether a stream
carries sound.

**Join and unjoin.** `media_player.join(kitchen, [den, patio])` sends
`{"v":2,"t":"join","zone":"den","target":"kitchen"}` and the same for the patio: the catalog
says the room then plays in the target's group, forming a live group when the target was
alone, which is take the room (K78) from the member's side. Every member is validated before
the first command; a member already in the group sends nothing; a saved group or a foreign
entity is refused with a translated `ServiceValidationError`. `unjoin(den)` sends
`{"v":2,"t":"take","target":"den"}`: `take` on a room moves it into the group named for it
and dissolves a live group left with one room.

**Sources, on and off, transport.** The source list is the server's `stream` and the line-ins
offered now, by label. `select_source`, `turn_on` (`stream`) and `turn_off` (`none`) are
`take` for the entity's target, so on a saved group they assemble it. Pause, play, next and
previous are offered only while the group's source is a Spotify receiver, the one thing
`playback` reaches. There is no stop: the catalog has none, and `none` is "off".

**Announce and play_media.** With `announce`, a `media-source://` id is resolved inside Home
Assistant, the URL is made absolute with `async_process_play_media_url`, and the integration
refuses any URL whose scheme, host and port are not those of Home Assistant's internal or
external URL before anything is sent; then `announce` goes to the server, which refuses by
its own list. Without `announce`, only `chorus://input/stream` and
`chorus://input/<endpoint>/<input>` are played. `browse_media` is one directory of those
inputs. Home Assistant's media sources are not passed through in browsing: an item browsed
there could only be announced, never played, and a browser that offers what cannot be played
is worse than none.

**Refusals.** An `error` becomes a translated `HomeAssistantError` chosen by `field` (`url`,
`target`, `source`, `group`, `zone`, `volume`, `t` for "no player", else a general one), with
the server's `detail` as a placeholder. A `url` refusal of an announcement makes the
integration read `GET /api/server` again; when this Home Assistant's origin is not in
`announce_origins` the error says so and names `--announce-origin` (the list is read at the
moment of the refusal, never trusted from setup, because a restart can change it); a `refused` (426) and a server without catalog 2 become a repair
issue.

**No endpoint.** The integration registers no HTTP view, no webhook, no static path and no
script. `tests/test_no_unauthenticated_endpoint.py` would catch the first one done wrong, at
run time and statically, and proves its checker on bad examples.

**Diagnostics** are built from named fields of the parsed state, not by redacting the raw
state: a field the catalog adds later cannot leak by default.

## Not chosen

- **v1 `ungroup` for unjoin.** It moves the room and "dissolves nothing", leaving a live
  group of one room. `join` has no form meaning "leave".
- **`take` with `source` `none` for unjoin** (the research's "leaves and goes idle"). Leaving
  a group and choosing what the room plays next are two things; `take` without a source lets
  the server's own rule say what the room plays.
- **Select source on the room's group** (as Sonos does). The coordinator's design says the
  entity's target; a saved group's player is how a whole group is switched.
- **A PyPI client** and **a requirement in the manifest**: both need a public repository, and
  a requirement is installed into the Home Assistant container at run time.
- **A WebSocket transport**: a second subscriber protocol (ADR 0026).
- **Redacting the raw state in diagnostics**: a denylist misses the next field.
- **A coverage floor only**: the config flow is held at 100 % as well, since the Bronze rule
  asks for full coverage of the flow.

## Consequences

- The identity, `announce` and its refusals are read from the shared vectors of ADR 0136
  (`fixtures/control/v2/server*.json`, `announce*.json`, `error-announce-*.json`); the tests
  keep no copy.
- What is ASSUMED until `tests/test_live_server.py` has run against the real server
  (`CHORUS_SERVER_BIN=<path> make ha-live`; this track built no server and did not run it):
  the server's command line in that test (the tone source with `--slots 4`); that `join` to
  a room in a saved group keeps the saved group's id (the tests' fake does); what a room
  plays after a `take` without a source; that a 1.5 s WAV is seen as `Announcement` in the
  state for long enough to be observed.
- An announcement that the server accepts and then cannot fetch is invisible to Home
  Assistant (the command was answered 200; ADR 0136). The README says so.
- The gate has two new steps, `ha-integration` (both tiers) and `ha-hassfest` (full tier),
  each under a minute warm; both fetch once and then need no network.
- Default entity ids repeat the room's name (area, then device) on Home Assistant 2026.9;
  that is Home Assistant's naming applied to the owner's choice of suggested areas.
- Left to goal 19: speaker devices and every other entity (sound controls, firmware,
  diagnostics sensors, buttons), the homelab's pinned copy and read-only mount, the voice
  path. Left to goal 20: the dashboard and anything served to the frontend, which is where
  the first HTTP view or static path would appear and where the endpoint test's allowlist
  would first be used.
