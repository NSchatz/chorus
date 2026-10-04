# chorus for Home Assistant

A custom integration that puts a chorus house into Home Assistant: every room and every saved
group is a media player, live groups are the room players' members, and Home Assistant can
join and unjoin rooms, set room and group volume, pick inputs and play announcements.

It talks to `chorus-server`'s control plane (`docs/control-plane.md`) over the local network:
HTTP for commands and one server-sent event stream for state. Nothing goes through a cloud,
the state is never polled (only the speakers' diagnostic sensors are, once a minute, while one
is enabled), and the integration installs no Python package into Home Assistant.

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

- Every **adopted speaker** of that server: one device, named for the speaker, linked to its
  room's device while it has a room and to the server otherwise. A speaker that takes
  firmware updates and has said what it runs has a firmware `update` entity, every
  speaker has its diagnostic sensors, and a speaker with buttons (the compact speaker and
  the streaming amp) has one `event` entity per button.

An older server, or one that only speaks catalog version 1, is refused
at setup with a repair issue saying so.

## Use cases

- Whole-house or per-floor audio from a dashboard: turn a saved group on, set its volume.
- "Play the kitchen's music in the den too": `media_player.join`.
- Automations that announce (a doorbell, a timer, a spoken message) in a room or a saved
  group, with the announcement's own volume.
- Switching a room to a line-in (a turntable, a TV, a streamer box) by name.
- Night mode and speech enhancement on a schedule or from a scene (a film night, a sleeping
  child), and a room's bass and treble from a dashboard.
- Seeing which speakers run which firmware and that a staged image is waiting, and installing
  it on one speaker from its device page, watching the transfer.
- Seeing how a speaker reaches the network and what it runs, and, for the ones you enable,
  charting its signal strength, buffer and drift while you chase a dropout.
- Automations that start from a speaker's own buttons: a long press of play/pause in the
  kitchen that also turns the lights on, a "next" press that advances something chorus does
  not play.
- A lamp that follows the music: a room's Visualizer sensor carries the colour, the level
  and the beat of what the room plays, for an automation of yours to map to a light.
- Switching a room's quiet hours off for a party, or its line-in autoplay off while a
  turntable is being set up.

## Installation instructions

The integration is one directory: `custom_components/chorus`. HACS is not used: it is
installed as a pinned copy of one chorus commit.

1. From a chorus checkout, `tools/ha-export.sh <dir> <commit>` writes `<dir>/chorus` (the
   directory) and `<dir>/chorus.lock` (the commit and the sha256 of every file). Keep both
   with your Home Assistant configuration and mount `chorus` read-only at
   `custom_components/chorus` in Home Assistant's configuration directory, or copy it there.
   `tools/ha-export.sh --verify <dir>`, or `sha256sum --check --strict chorus.lock` without a
   checkout, says whether the copy is still the commit's. The owner's installation carries
   the copy in its own repository with a read-only bind (`docs/home-assistant.md`,
   "Installing: the pinned copy").
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

### Updating

Nothing updates by itself. A new version is a new pin: export a later commit over the copy
(`tools/ha-export.sh <dir> <commit>` replaces it whole), check it, and restart Home
Assistant. Take a new pin when Home Assistant itself is updated (the integration is tested
against one Home Assistant version, the one `chorus.lock` names), when the chorus server is
updated past what the integration speaks (a repair issue says so), or for a fix.

## Configuration parameters

There are none: the integration has no options. To change the host or the port, use
**Reconfigure** on the entry; it refuses an address that belongs to a different server.

## Removal instructions

1. **Settings > Devices & services > chorus > the entry's menu > Delete.**
2. Remove the `custom_components/chorus` directory (or the mount) and its `chorus.lock`, and
   restart Home Assistant.

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

### Sound controls (one set per room, on the room's device)

| Entity | What it is |
|---|---|
| Bass, Treble (`number`) | The room's tone, in whole dB from -10 to 10. A setting |
| Loudness, Night mode, Speech enhancement (`switch`) | The three switches of the room's sound. Loudness is on by default, the other two off. Settings |
| Input (`select`) | What the room plays, picked from the inputs the server offers now: the server's stream, then each line-in by its label. Picking one takes the room and plays it, exactly as the media player's select source does. It shows nothing while the room plays something that is not an input (a cast, a Spotify receiver, nothing) |
| Quiet hours (`switch`) | Whether the room's quiet-hours windows cap its volume. Off keeps every window as it was set and caps nothing; on again inside a window applies the cap at once. The windows themselves are set in chorus. A setting |
| Autoplay *input* (`switch`, one per autoplay rule) | Whether the rule that starts an input in this room when its signal arrives is enabled. It is on the device of the room the rule targets, or of the saved group when the rule targets one. Rules are made and deleted in chorus: a new rule gets its switch at once and a deleted rule's switch is removed. A setting |

Each change is one command that carries only what changed, so setting the bass never touches
the treble. Bass management, room EQ, the quiet-hours windows, alarms and sleep timers are
not entities.

### Speaker firmware (`update`, one per speaker that reports its firmware)

Each adopted speaker is a device. A speaker that takes updates and has reported what it runs
has a **Firmware** update entity on it (a speaker that has not, or cannot take updates, has a
device and no such entity).

| What it shows | Meaning |
|---|---|
| Installed version | The version the speaker runs |
| Latest version | The version of the staged image the server **verified** for this speaker's board, when it is above the installed one; otherwise the installed version. An image the server refused (a digest that does not match, a bad manifest, ...) is never shown here, and neither is an older one |
| On / off | On while such an image is waiting |
| In progress, percentage | An install is under way; the percentage is the bytes the speaker has received of the image's size. While the speaker verifies, reboots and runs the new image on trial there is no percentage |
| Install state, Reason | What the speaker is doing (`requested`, `receiving`, `verified`, `pending_verify`) or how the last install ended: `confirmed`, `rolled_back` (the new image did not rejoin the server and the previous one runs again), `refused`, `interrupted`, `cancelled`, with the reason the speaker or the server gave |
| Image, Image version | The staged image the state is about, and its version (after a rollback: the version that was tried) |

**Nothing installs on its own.** The integration sends the server's install command in exactly
one place: the entity's **Install** action (`update.install`). Setting the integration up,
reloading it, a reconnect, a restart of Home Assistant or of the server, and a newly staged
image send nothing to anybody. The action installs the image shown as the latest version on
that one speaker; there is no "install all", no choosing a version and no reinstall here.
The integration never calls the action itself: an automation of yours that calls
`update.install` is an explicit install, and yours.

**Installing on a real speaker is the owner's action at the bench.** The server refuses to
send an image to a speaker that is not on the server's own host unless it was started with
`CHORUS_OWNER_AT_BENCH` set to `1` in its environment (`docs/firmware-updates.md`). Without it the
Install action fails with a message saying so and nothing is sent to the speaker; everything
the entity shows still works.

### Speaker diagnostics (`sensor`, eight per adopted speaker)

What a speaker reports about itself, read from the server's `GET /metrics`
(`docs/telemetry.md`). All eight are in the **Diagnostic** section of the speaker's device.
Two are enabled when the speaker is added; the others are there, disabled, for you to enable
on the device page when you want them.

| Sensor | What it is | Enabled by default |
|---|---|---|
| Link | How the speaker says it reaches the network: Wired, Wi-Fi or Unknown | **Yes** |
| Firmware version | The version the speaker last said it runs. Kept while the speaker is disconnected | **Yes** |
| Sync error | The speaker's own estimate of its playout error against the server's timeline, signed, in microseconds. Not a measured error between two speakers | No |
| Buffer fill | Audio queued ahead of the speaker's playout point, in milliseconds | No |
| Rate correction | The rate correction the speaker's clock servo has in force, signed, in ppm. A gauge, not a count of corrections | No |
| Resyncs | Hard resynchronisations since the speaker's session began. It starts again at 0 when the speaker reconnects | No |
| Signal strength | Wi-Fi signal strength in dBm. Unknown for a wired speaker | No |
| Temperature | Degrees Celsius. Unknown on every speaker today: no board has a temperature sensor yet | No |

**The poll interval is 60 seconds**: one request for all speakers. While no diagnostic sensor
is enabled the server is not asked at all. Asking for an update sooner (the
`homeassistant.update_entity` action) does not ask the server sooner: within 55 seconds of
the last request the last answer is used.

A value the speaker does not have is **unknown**, never zero. While a speaker is disconnected
only its firmware version is kept; the others are unavailable.

Not carried by the server's exporter, and so not here (follow-ups on the server and the
firmware, nothing is made up in their place): a **count of corrections** (the exporter has the
correction in force, shown as Rate correction, and no count), and a **real temperature** (the
series exists, and no speaker has a sensor to fill it).

### Speaker buttons (`event`, five per speaker that has buttons)

A speaker that declares the controller role (the compact speaker and the streaming amp: the
classes with front buttons) has five event entities on its device, one per button. Each fires
once for every press of that button the chorus server accepted, and at no other time.

| Entity | Event types | The press the speaker sent (`command`) |
|---|---|---|
| Play/pause button | `press`, `long_press` | `press`: `toggle`. `long_press`: `leave` when the room plays in a group, else `join` (its `target` is the group) |
| Volume up button | `press` | `volume_step` with a `value` above zero. A held button repeats, and each repeat is a press |
| Volume down button | `press` | `volume_step` with a `value` below zero. A held button repeats, and each repeat is a press |
| Next button | `press` | `next` |
| Previous button | `press` | `previous` |

Every event carries, beside `event_type`: `command` and `value` (what the button asked the
server for), `target` (the group of a `join`, otherwise empty), `room` (the room the speaker
plays in) and `outcome` (`applied` when the press changed the room, `waits-for-an-input` for
play/pause, next and previous, which act on what the room plays).

The entities only report. What a button does in chorus is unchanged and is the server's: the
integration sends nothing when one is pressed. A press the server refused is no event.

**A reconnect fires nothing.** The server keeps no press and the integration keeps none
either: a press made while Home Assistant was not connected to the server is never delivered
later, and coming back from unavailable is not an event. While the integration cannot hear
presses the button entities are **unavailable**.

A press from something that is not one of these buttons (a wall remote that is not an adopted
speaker, a command no speaker button sends) is ignored, with one debug log line.

### Room visualizer (`sensor`, one per room, disabled by default)

The colour, the level and the beat of what a room plays, for an automation to map to a light
(`docs/visualizer.md`). It is on the room's device, named **Visualizer**, and it starts
**disabled**: enable it on the room's device page for the rooms whose lights should follow.
While it is disabled the integration holds no visualizer stream open to the server for that
room.

| | |
|---|---|
| State | The level, in percent: the frame's peak (0 to 255, a 60 dB span) as a percentage |
| `rgb_color` | The colour, `[red, green, blue]`, each 0 to 255, as `light.turn_on` takes it |
| `brightness` | The colour's brightness, 0 to 255, as `light.turn_on` takes it |
| `transition` | How long the light should take to reach the colour, in seconds, as `light.turn_on` takes it (the server says 0.5) |
| `beat` | 0, or the strength of a beat, 1 to 255 (128 and up is a strong one). Shown in one state and gone in the next |
| `lead_ms` | How many milliseconds after the server wrote the frame the room hears it. Zero or negative: heard already |

**The rate cap: at most one state every 200 ms, five a second**, whatever the server sends
(the server itself sends at most ten frames a second). The latest frame is the one shown, never
a backlog; a beat that fell between two states is shown in the next one.

**Idle.** While the room plays nothing, or plays silence, the sensor is idle: the state is `0`,
`rgb_color` is `[0, 0, 0]`, `brightness` is 0 and `beat` is 0. It is idle as soon as the
server says the room went silent, or two seconds after the last frame when the server just
stops sending (the room was given nothing to play). A light mapped to it goes off:
`light.turn_on` with a brightness of 0 turns a light off.

**The recorder.** The sensor has no state class, so Home Assistant never compiles long-term
statistics for it, and none of its attributes is recorded (no colour, no beat). Its state (the
level) still goes into the recorder's short-term history like any entity's, at most five rows
a second while the room plays, until the recorder purges it (after ten days by default). An
integration cannot keep an entity's state out of the recorder; you can:

```yaml
recorder:
  exclude:
    entity_globs:
      - sensor.*_visualizer
```

The sensor only reports. The integration drives no light: the mapping is your automation
("Examples" has one). While its stream is not attached the sensor is **unavailable**.

## Data updates

Local push. The integration holds one event stream (`GET /api/events`) per server and the
server sends the complete state on every change; entities update at once. Commands are
answered with the new state, which is applied immediately. If the stream is lost, entities
become unavailable and the integration reconnects with an exponential backoff from one second
to one minute, with jitter. A stream that has been silent for 45 seconds is checked once with
`GET /api/state`; that is a liveness check, not polling.

Button presses arrive the same way on a second stream (`GET /api/controller-events`), which
carries one message per accepted press and never a past one. It reconnects by the same
backoff and is checked for liveness the same way; while it is not attached the button
entities are unavailable (one log line says so) and nothing else is affected.

An enabled Visualizer sensor holds one more stream of the same kind, its room's
(`GET /api/visualizer?zone=<room>`), with the same backoff and liveness check. A disabled one
holds none. The server pushes at most ten frames a second on it and the sensor writes at most
five states a second.

The speakers' diagnostic sensors are the one exception, because the server's state carries no
telemetry: they are polled from `GET /metrics` every 60 seconds, by one request for all
speakers, and only while at least one of them is enabled. If that request fails, or the
answer is not the server's metrics text, the diagnostic sensors become unavailable (one log
line says so) until the next good answer; nothing else is affected.

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

Turn the kitchen lights on with a long press of the kitchen speaker's play/pause button (the
speaker still joins or leaves its group, as it always does):

```yaml
automation:
  - alias: Kitchen speaker long press turns the lights on
    triggers:
      - trigger: state
        entity_id: event.kitchen_kitchen_left_play_pause_button
        not_from: unavailable
    conditions:
      - condition: state
        entity_id: event.kitchen_kitchen_left_play_pause_button
        attribute: event_type
        state: long_press
    actions:
      - action: light.turn_on
        target:
          entity_id: light.kitchen
```

`not_from: unavailable` matters: an event entity's state is the time of its last event, and
it shows that time again when it comes back from unavailable, which is not a press.

Have the den's lamp follow the den's music (enable the den's **Visualizer** sensor first: it
starts disabled):

```yaml
automation:
  - alias: Den lamp follows the music
    mode: restart
    triggers:
      - trigger: state
        entity_id: sensor.den_visualizer
    conditions:
      - condition: template
        value_template: "{{ has_value('sensor.den_visualizer') }}"
    actions:
      - action: light.turn_on
        target:
          entity_id: light.den_lamp
        data:
          rgb_color: "{{ state_attr('sensor.den_visualizer', 'rgb_color') }}"
          brightness: "{{ state_attr('sensor.den_visualizer', 'brightness') }}"
          transition: "{{ state_attr('sensor.den_visualizer', 'transition') }}"
```

The trigger has no `to` or `from`, so it also fires when only the colour changed. The
condition skips the moments the sensor is unavailable, when it has no colour. When the room
goes quiet the sensor's brightness is 0, and `light.turn_on` with a brightness of 0 turns the
light off. For a flash on the beat, add a condition on the `beat` attribute (128 and up is a
strong one).

## Known limitations

- A button press made while Home Assistant is not connected to the chorus server is lost:
  the server keeps none. Two-way speakers and subwoofers have no front buttons and no button
  entities; a pairing button and a microphone mute switch are local to the speaker and are
  not events. A wall remote that is not an adopted speaker has no entities.

- The Visualizer sensor says when the room HEARS a frame only as `lead_ms`; it does not wait
  for it. A state is written when the frame arrives (or up to 200 ms later, held by the rate
  cap), and Home Assistant, the light's integration and the lamp each add their own delay.
  Nothing here has been measured against a lamp: do not expect a light to land on the beat,
  least of all in a wired room, where a frame arrives only a few tens of milliseconds before
  it is heard.
- The Visualizer carries one colour and one level for the room, no spectrum. Rooms of one
  group show the same frames.

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
- The Input select, like select source, takes the room out of its group.
- An autoplay switch is named for its input's label as it was when the switch was created; a
  label changed later shows after the integration is reloaded. A rule given another target
  is a new switch on the other device, and the old one is removed.
- Quiet hours and autoplay rules can be switched here, not edited: their windows, inputs and
  targets are set in chorus.
- The firmware entity installs only an image above the running version. Going back to an older
  image, installing the same version again, installing on every speaker at once, cancelling a
  transfer and re-reading the firmware directory are the server's own commands
  (`docs/firmware-updates.md`), not actions here.
- After a rollback the image is still staged, so the update is still shown as available:
  remove or replace the image on the server. Skipping a version in Home Assistant hides it
  there only.
- A speaker's device takes its room's name as a suggested area when it is first seen; a
  speaker moved to another room afterwards is linked to the new room's device, and its area
  is yours to change.
- On Home Assistant 2026.9 a new entity's id is built from the area, the device and the
  entity name. The room's name is suggested as its area, so a default id reads
  `media_player.kitchen_kitchen`; rename the entity or the area to taste. (The examples above
  use the short form.)
- The diagnostic sensors are at most a minute old, and they are what each speaker says about
  itself: the Sync error is not a measurement of how far apart two speakers are. For history
  at a second's resolution, scrape the server's `/metrics` with Prometheus
  (`docs/telemetry.md`).
- There is no sensor for a speaker's underruns or free heap; the server's `/metrics` has them.
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
| "The chorus server refused the sound setting" | The server did not accept a bass, treble, loudness, night mode or speech value; the message carries the server's own reason |
| An autoplay switch disappeared | The rule was deleted in chorus, or given another target (its switch is then on that room's or saved group's device) |
| The Input select shows nothing | The room plays something that is not an input: a cast, a Spotify receiver, or nothing |
| "The chorus server installs firmware on a real speaker only while the owner is at the bench" | The speaker is not on the server's own host and the server runs without `CHORUS_OWNER_AT_BENCH` set to `1`. Nothing was sent. Installing on a real speaker is the owner's action |
| "The speaker already has a firmware install in progress" | An install is under way for that speaker (the entity shows it); wait for its outcome |
| "The chorus server did not verify the staged image, or it changed on disk since" | The image file is not the one the server verified. Stage it again and send the server `firmware_rescan` |
| A speaker has a device and no Firmware entity | It has not reported what it runs: it is absent since the server started, or its firmware takes no updates |
| A speaker's diagnostic sensors are all unavailable while its media players work | The server's `/metrics` did not answer, or answered with something that is not its metrics text (a proxy in between, an older server). The log has one line with the reason |
| Most of a speaker's diagnostic sensors are unavailable and its Firmware version is not | The speaker is disconnected |
| Signal strength or Temperature says "Unknown" | The speaker has no such value: it is wired, or has no temperature sensor |
| A speaker's diagnostic sensor is missing from the dashboard | All but Link and Firmware version start disabled: enable it on the speaker's device page |
| A room has no Visualizer sensor on the dashboard | It starts disabled: enable it on the room's device page |
| The Visualizer sensor is unavailable while the room's media player works | Its stream is not attached: the server is older than the route (`GET /api/visualizer`), or the stream was lost and is being opened again. The log has one line with the reason |
| The Visualizer sensor stays at 0 while music plays | The server computes no visualizer stream in its one-stream shape (`--slots 0`), and none for a room whose group plays nothing |
| The Firmware entity says "Rolled back" | The new image did not rejoin the server in time, so the speaker started the previous one again. Nothing retries it |

Download diagnostics from the entry's menu when reporting a problem: the file holds no host,
URL, key or stored source.

## Development

```sh
make ha-test        # ruff, mypy --strict, pytest under the pinned harness, coverage
make ha-hassfest    # Home Assistant's hassfest, as core and as custom
make ha-test HA_TEST_ARGS="-k announce"   # a narrowed run: the selected tests alone
tools/ha-export.sh <dir> [<commit>]       # the pinned install copy and its lock
```

Both need `uv` (pinned in `mise.toml`) and keep their virtual environment outside the
repository; without it they fail naming it, under CI too. A narrowed run holds no lint, types
or coverage threshold and ends with `ha-test: NARROWED PASS`; only the whole run is the gate's. `docs/home-assistant.md` has the details.
