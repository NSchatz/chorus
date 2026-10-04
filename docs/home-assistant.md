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
  `Content-Type: application/json` and no `Origin` header.
- **No runtime requirement.** The client library is vendored at
  `custom_components/chorus/_aiochorus/`: async only, fully typed (`py.typed`), no Home
  Assistant import, an injected `aiohttp.ClientSession`. That directory is its one source
  location. The manifest's `requirements` is `[]`, so Home Assistant never installs a package
  for chorus.
- Written to every rule of the Bronze, Silver, Gold and Platinum tiers of Home Assistant's
  Integration Quality Scale as core 2026.9.3 lists them (54 rules), self-certified in
  `quality_scale.yaml`: 44 done, 10 exempt, each exemption with its reason. A custom
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
| A room (zone) | One device named for the room, `suggested_area` its name, with a `media_player` and a group-volume `number` |
| A saved group | One device under the server with a `media_player`, always present |
| A live group | No entity: the `group_members` of its rooms' players, leader first |
| A speaker | Nothing yet (goal 19) |

Home Assistant's areas are Home Assistant's: the integration only suggests one and never
creates, renames or assigns an area.

Rooms and saved groups that appear while Home Assistant runs get their devices and entities
at once; ones that disappear have their devices removed; a renamed room or group renames its
device.

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

A volume is Home Assistant's 0..1 level as the catalog's amplitude factor, written with
exactly three decimals. The server clamps every volume to the room's limits; the entity shows
what the state says afterwards, never what was asked.

**Join is "take the room"** (K78): the member leaves whatever it played. `join` with a room
as its `target` is the catalog command whose documented meaning is exactly that, and it forms
a live group when the leader was alone. **Unjoin is `take` on the room**: the catalog says
`take` moves the room into the group named for it and dissolves a live group left with one
room. Catalog v1's `ungroup` also moves the room but "dissolves nothing", which would leave a
one-room live group behind; `join` has no form that means "leave".

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
   player, or none free), and a general one for any other. When an announcement's `url` is
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
```

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
