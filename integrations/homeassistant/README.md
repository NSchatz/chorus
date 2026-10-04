# chorus for Home Assistant

A custom integration that puts a chorus house into Home Assistant: every room and every saved
group is a media player, live groups are the room players' members, and Home Assistant can
join and unjoin rooms, set room and group volume, pick inputs and play announcements.

It talks to `chorus-server`'s control plane (`docs/control-plane.md`) over the local network:
HTTP for commands and one server-sent event stream for state. Nothing goes through a cloud,
nothing is polled, and the integration installs no Python package into Home Assistant.

How it maps chorus to Home Assistant, the security rules and the test harness are in
[`docs/home-assistant.md`](../../docs/home-assistant.md); the decisions are in
[ADR 0138](../../docs/decisions/0138-the-home-assistant-integration.md).

## Supported devices

- A **chorus server** speaking control catalog version 2 that answers `GET /api/server`
  (chorus-server from goal 18 on). It appears as one service device. For announcements it
  needs `--players` (at least one) and `--announce-origin <this Home Assistant's URL>`.
- Every **room** of that server: one device, named for the room, with the room's name
  suggested as its area.
- Every **saved group** of that server: one device under the server.

Not supported (yet): the speakers themselves as devices, and everything about them (firmware,
diagnostics, buttons). An older server, or one that only speaks catalog version 1, is refused
at setup with a repair issue saying so.

## Use cases

- Whole-house or per-floor audio from a dashboard: turn a saved group on, set its volume.
- "Play the kitchen's music in the den too": `media_player.join`.
- Automations that announce (a doorbell, a timer, a spoken message) in a room or a saved
  group, with the announcement's own volume.
- Switching a room to a line-in (a turntable, a TV, a streamer box) by name.

## Installation instructions

The integration is one directory: `custom_components/chorus`. HACS cannot install it (the
repository is private).

1. Copy `integrations/homeassistant/custom_components/chorus` into your Home Assistant
   configuration directory as `custom_components/chorus`, or mount it there read-only (the
   homelab carries a pinned copy and a read-only bind from goal 19 on).
2. Restart Home Assistant.
3. **Settings > Devices & services > Add integration > chorus**, or accept the discovered
   server if Home Assistant found one.

Nothing else is installed: the manifest lists no requirements, and the client library is
inside the directory.

### Installation parameters

| Parameter | What it is |
|---|---|
| Host | The hostname or IP address of the chorus server |
| Control port | The port of the server's control plane, its `--control-listen` port. Default `4020`. Not the audio port (`4010`) |

Discovery (`_chorus-ctl._tcp.local.`) fills both in; it is a convenience, and the form is the
primary way, because multicast does not cross every network. The server's own identifier
(from `GET /api/server`, also in the advertisement's `id=`) is what makes an entry unique: the
same server cannot be added twice, and a server discovered at a new address updates its entry.

## Configuration parameters

There are none: the integration has no options. To change the host or the port, use
**Reconfigure** on the entry; it refuses an address that belongs to a different server.

## Removal instructions

1. **Settings > Devices & services > chorus > the entry's menu > Delete.**
2. Remove the `custom_components/chorus` directory (or the mount) and restart Home Assistant.

Nothing is left on the chorus server: the integration stores nothing there.

## Supported functions

### Room media player (`media_player`, one per room)

| Function | What it does on the server |
|---|---|
| Volume set, up, down, mute | The room's volume and mute. The server clamps to the room's limit and quiet hours, and the entity shows what the server says afterwards |
| Join (`media_player.join`) | Each named room leaves what it played and plays in this room's group ("take the room"). Rooms already there are left alone. A saved group cannot be joined |
| Unjoin (`media_player.unjoin`) | The room leaves for a group of its own; a live group left with one room dissolves |
| Group members | The rooms of the room's group, leader first (the group's first room in the server's order); a room alone lists itself |
| Select source | Takes the room and plays the server's stream or one of the line-ins offered now (shown by its label where it has one) |
| Turn on, turn off | Takes the room and plays the server's stream, or nothing |
| Play media | `chorus://input/stream` or `chorus://input/<endpoint>/<input>` (the same as select source). With `announce: true`: a clip Home Assistant itself serves (a `media-source://` item, a TTS proxy URL), with an optional `extra: {volume: 0..1}` |
| Browse media | The chorus inputs, each playable. chorus has no content of its own to browse |
| Pause, play, next, previous | Only while the room's group plays a Spotify receiver (the one source the server can control) |
| Now playing | Title, artist, album, artwork, duration and state, when the server knows them |

State: `off` (plays nothing), `playing` / `paused` / `buffering` (the server says what is
playing), `idle` (a network player with nothing reported), `on` (the stream or a line-in).

### Saved group media player (`media_player`, one per saved group)

Always present, assembled or not. Its volume is the Sonos-style group volume while it is
assembled (setting it scales every room and keeps their balance); mute mutes every room;
select source, turn on, play media and announce **assemble** the group. Attributes: `rooms`
(the rooms' entity ids) and `active`. It has no join: its membership is edited in chorus.

### Group volume (`number`, one per room)

The group volume of whatever group the room plays in, in percent. Available only while the
room plays with at least one other room.

## Data updates

Local push. The integration holds one event stream (`GET /api/events`) per server and the
server sends the complete state on every change; entities update at once. Commands are
answered with the new state, which is applied immediately. If the stream is lost, entities
become unavailable and the integration reconnects with an exponential backoff from one second
to one minute, with jitter. A stream that has been silent for 45 seconds is checked once with
`GET /api/state`; that is a liveness check, not polling.

## Examples

Announce in the kitchen at 30 %:

```yaml
action: media_player.play_media
target:
  entity_id: media_player.kitchen
data:
  media_content_type: music
  media_content_id: media-source://tts/tts.home_assistant_cloud?message=Dinner+is+ready
  announce: true
  extra:
    volume: 0.3
```

Play the turntable everywhere downstairs:

```yaml
action: media_player.select_source
target:
  entity_id: media_player.downstairs
data:
  source: Turntable
```

Bring the den and the patio into the kitchen's music, then let the den go:

```yaml
- action: media_player.join
  target:
    entity_id: media_player.kitchen
  data:
    group_members:
      - media_player.den
      - media_player.patio
- action: media_player.unjoin
  target:
    entity_id: media_player.den
```

## Known limitations

- chorus plays inputs and announcements. `play_media` with a stream URL, a radio station or
  any other media source is refused: content is chosen in the apps that cast to chorus.
- Announcements are fetched by the chorus server from Home Assistant, so the server must be
  able to reach Home Assistant's internal or external URL, and that URL must be on the
  server's own list (`--announce-origin`). An address anywhere else is refused twice: by the
  integration before anything is sent, and by the server.
- An announcement interrupts and restores; it is not mixed over the music (no ducking yet).
  The stream, a line-in and a chime come back afterwards. A UPnP cast or a Spotify receiver
  does **not** come back: the group plays nothing after the clip and is started again from
  the app that drove it.
- An announcement in a room that is in a group is heard by the whole group. One for a saved
  group that is not assembled assembles it, and it stays assembled.
- With a volume, every room of that group plays the clip at it (clamped to each room's
  limit) and gets its own volume back afterwards.
- A clip the server cannot fetch or decode (a 404, an unsupported file) fails **silently**
  as far as Home Assistant can tell: the server accepts the command and the reason is only
  in the server's log.
- An announcement needs a free player on the server (`--players`); while every player is in
  use it is refused, and so is one for a room where an alarm is ringing.
- There is no stop, seek, shuffle or repeat: the server has no such command. Pause, play,
  next and previous exist only for a Spotify receiver.
- A saved group's volume can be set only while it is assembled.
- Selecting a source on a room that is in a group takes the room out of the group (that is
  what "take the room" means). To change what a whole group plays, select the source on the
  saved group, or on a room and join the others again.
- Live groups are not entities; they are the room players' members.
- On Home Assistant 2026.9 a new entity's id is built from the area, the device and the
  entity name. The room's name is suggested as its area, so a default id reads
  `media_player.kitchen_kitchen`; rename the entity or the area to taste. (The examples above
  use the short form.)
- The control plane has no authentication (it is a local-network service by design), so there
  is nothing to re-authenticate.

## Troubleshooting

| What you see | What it means |
|---|---|
| "The chorus server did not answer" in the form | Wrong host or port, the server is not running, or a firewall. The port is the control port (`4020`), not the audio port |
| "Something answered, and it was not a chorus control plane" | Another service is on that port |
| A repair issue "The chorus server speaks an unsupported control catalog" | The server is older than this integration needs. Update the server (or install the matching integration), then reload the entry |
| Every entity is unavailable | The event stream was lost. The log has one line saying so and one when the server is back; the integration reconnects on its own |
| "The chorus server does not announce from this Home Assistant" | Start chorus-server with `--announce-origin` set to the URL in the message (Home Assistant's internal or external URL, scheme, host and port) |
| "The chorus server refused the announcement's address" | The address is on the server's list but not of a shape it plays (the message carries the server's words) |
| "The chorus server cannot do that right now" on an announcement | The server runs no player (`--players`), or every player is in use |
| An announcement is accepted and nothing is heard | The server could not fetch or decode the clip; the reason is in the server's log |
| After a server restart Home Assistant offers the server as a new device and the old entry cannot connect or reconfigure ("a different chorus server") | The server runs with `--ephemeral-identity`, which gives it a new id at every start. Run it with a kept identity (`--identity-dir`) |
| "chorus announces only media that Home Assistant itself serves" | The media id resolved to an address that is not this Home Assistant's internal or external URL. Set the internal URL under Settings > System > Network |
| The group volume number is unavailable | The room is playing alone |

Download diagnostics from the entry's menu when reporting a problem: the file holds no host,
URL, key or stored source.

## Development

```sh
make ha-test        # ruff, mypy --strict, pytest under the pinned harness, coverage
make ha-hassfest    # Home Assistant's hassfest, as core and as custom
```

Both need `uv` (pinned in `mise.toml`) and keep their virtual environment outside the
repository. `docs/home-assistant.md` has the details.
