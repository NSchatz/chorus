# Inputs: alarm sources, line-in sharing and streamers

What a room can play that is not the server's configured stream, as it stands after goal 17
(decided in `docs/decisions/0129-stored-alarm-sources-line-in-sharing-and-streamer-inputs.md`): the four alarm sources of K80, a line-in shared to
any number of groups, and a network streamer wired to a line-in. The commands are the control
catalog's (`docs/control-plane.md`); the command line is `chorusctl` (`docs/chorusctl.md`).

## The security rule

Brief section 4.8, quoted:

> No arbitrary URL fetch except the input paths the decisions name (UPnP renders, HA's media and
> TTS URLs from HA's own address, stored alarm stream URLs).

So the server has no command that plays a URL in a room, and it never will get one by accident:

- A URL reaches the server through the control API in exactly one way: `source_store`, which
  stores it under a name and plays nothing.
- A stored URL is played in exactly one situation: an alarm whose `source` is `stored:<id>`
  rings. A `take` naming `stored:<id>` is refused by name, and the room model refuses the
  spelling as a group's source however it is asked.
- The scheme is `http` or `https` (anything else is refused when it is stored), and the fetch is
  held to the server's fetch policy when it is played (`docs/streams.md`): never this machine's
  loopback, never one of this server's own ports, redirects followed under the same rule, a
  resolved address checked before it is dialled.
- The other two named paths are not this document's: the UPnP renderer (`docs/upnp.md`) and the
  home automation's URLs (a later goal).
- There is no microphone input: a source is a line-in, an optical input or HDMI ARC, and an
  input's role is `line-in` or `streamer` (the speaker microphone goes to the voice path only).

Stored sources are in the state message, values and all (`stored_sources`): whoever can read the
control API can read a stored URL. Do not store a URL with a password in it.

## The four alarm sources (K80)

An alarm's `source` is one spelling, and every one of them either plays or rings the `bell`
chime with the reason in the server's log (`schedule alarm=<id> fallback=chime reason=<why>`):
an alarm must still wake.

| Source | Spelling | Plays through | Rings the chime instead when |
|---|---|---|---|
| A chime | `chime:<name>` | the chime rendered at start, repeated with a gap (`docs/chimes.md`) | the name is not a chime (`unknown-chime`) |
| A line-in | `line-in:<endpoint>/<input>` | the input's port, shared with any group already playing it | it is not offered when the alarm fires (`not-offered`); its endpoint goes while it rings (`input-gone`) |
| A stored stream URL | `stored:<id>`, kind `url` | a network media player (`--players`), fetched under the fetch policy | below |
| A Spotify playlist | `stored:<id>`, kind `spotify` | a Soloist receiver, built by the Soloist server track | below; **off by default** |

Also accepted and never played: `none` (`source-none`), `player:<id>` (`player-source`: an
alarm has no media to hand a player), and a stored source that was edited out of the state file
(`not-stored`; the catalog refuses forgetting one an alarm plays).

Snooze is not a control command today (the schedule library has the arithmetic; nothing sends
it), so there is nothing source-specific to say about it. Stop (`alarm_stop`, `alarm_delete`,
the alarm's own `duration_min`, a person's command naming one of its rooms), the ramp from
silence, the two second end fade and the restore of each room's group, volume, mute and source
are the schedule runtime's and are the same for every source.

### An alarm with a stored source

```sh
chorusctl sources store morning-radio url "Morning radio" https://radio.example/stream.mp3
```

then an `alarm_set` with `"source":"stored:morning-radio"`. When it rings:

1. The alarm takes its target (K78) with the source `none`, sets its rooms to silence and
   starts its ramp, exactly as for a chime. The rooms are silent, not ringing yet.
2. In the same pass the conductor takes a free player and loads the URL
   (`PlayerSessions::play_held`, owner `alarm:<id>`, via `alarm`). No `--upnp` is needed: the
   players' reports are taken by the conductor when no renderer runs, and by the renderers'
   manager thread when one does. The log says `player p<i> plays for alarm:<id> on <target> via
   alarm`.
3. The alarm's group plays `player:p<i>` (`schedule alarm=<id> started stored=<sid>
   plays=player:p<i>`), and shows the stored source's name as what is playing (`now_playing`,
   `via` `alarm`), with the stream's own title after it when the stream names one.
4. When the alarm ends, the player is unloaded and given back (`player p<i> released for
   alarm:<id>`): an endless stream is closed, not left running.

It rings the chime instead, and keeps ringing, when:

| Reason in the log | What happened |
|---|---|
| `no-players` | the server was started without `--players` |
| `no-free-player` | every player is in use (a cast in another room, another alarm) |
| `url-refused` | the fetch policy refused the URL (the detail is the policy's own words) |
| `stream-failed` | the fetch or the decode failed, at once or later (`http status 404`, `unsupported: aac`, a connection that drops) |
| `stream-ended` | the stream ended while the alarm still rang (a file, not a station) |
| `not-started` | the room model would not give the group the player |

A failure that arrives later (the fetch runs on the player's thread) changes the group's source
from the player to the chime; the ramp is not restarted, so the chime comes in at whatever
volume the ramp has reached.

Known limit: a player still unloading from a cast the alarm itself displaced is not free yet,
so an alarm firing in a room that is casting, on a server with one player, rings the chime
(`no-free-player`). Give the server one player more than the casts it expects.

### The Spotify playlist source

`chorusctl sources store wake-list spotify "Wake up" spotify:playlist:<id>` stores and validates
the URI. Playing it is the Soloist server track's (Soloist is proprietary; chorus never ships
or downloads it). It ships **switched off**: with the switch off (every server today) the alarm
rings the chime with reason `soloist-off`. The seam is in the schedule runtime
(`Effect::PlaySpotify`, `Runtime::set_soloist_alarms`, and the two answers every stored source
uses, `on_alarm_source_started` and `on_alarm_source_failed`); with the switch on and no
receiver, the answer is `soloist-unavailable`.

## Line-in sharing

A line-in plays in any number of groups at once. `chorusctl inputs select <input> <target>` (a
`take` with a `line-in:` source) for a second target does not take the input away from the
first: both play it. An autoplay rule and an alarm naming an input another group already plays
play it too.

- The input is started once and stopped when no group plays it. A listener leaving does not
  restart or interrupt it for the others.
- Every group hears the same samples: the audio thread plays the input's port once a tick and
  every slot whose group plays the input sends that chunk, on the one grid, so two groups
  receive the same bytes under the same sequence and timestamp
  (`crates/server/tests/line_in_sharing.rs`).
- **One latency per input.** An input has one buffer and so one latency: the largest any
  listening group needs (180 ms for a wired group, the wireless tier's for a group with a
  wireless room). The source's own room plays at the local latency (30 ms) only while it is the
  input's sole listener and alone in its group. When a second group starts listening the
  latency grows without a glitch in the first room (ADR 0071's bounded time stretch, at most
  500 ppm: no frame dropped, repeated or inserted), and it comes back down the same way once
  the room is the sole listener again. A change asked for while one is still running waits for
  it to end.
- **A TV input** (optical, HDMI ARC) in low-latency mode goes from its hub to the TV's room as
  datagrams, off the slot grid, and a hub sends either those or the ordinary upstream, not both.
  So a TV input that a second group plays leaves low-latency mode for as long as it is shared
  (`tv-path mode=slot reason=shared`): every group, the TV's own room included, hears it on the
  slot path, in sync with one another at the shared latency, and the picture leads the sound
  in the TV's room by that latency. When the second group stops listening the TV's room goes
  back to low-latency mode by itself. To have other rooms follow the TV, that is the price; to
  keep lip sync in the TV's room, do not share its input.
- A line-in whose format is not the server's own is still refused
  (`line-in refused reason=format-mismatch`); the endpoint is told by the `stop` it is sent.

## A streamer on a line-in

A bought network streamer (the box that carries the licensed receivers chorus does not
implement, K60) wired to an endpoint's line-in is an input with a label:

```sh
chorusctl inputs label kitchen-amp/line-1 streamer "Kitchen streamer"
```

- **It plays when it has signal.** When the input's signal appears (the endpoint's own signal
  detection, ADR 0066) it plays into the room its endpoint is in, with no autoplay rule to
  write: goal 11's autoplay with the room as its target, the 30 s hold after the signal goes,
  and the restore of what the room played. An `autoplay` rule for the input, enabled or
  disabled, is the person's word and wins (another target, or never). Labelling an input
  that has signal now plays it at once.
- **The room shows it.** Every group playing the input carries a now-playing record with the
  label as its title and `via` `streamer`. What the streamer itself is playing is not known to
  chorus: it arrives as audio.
- **It is shared like any line-in**: `chorusctl inputs select kitchen-amp/line-1 <target>`.
  A group a person gave it to keeps it when the streamer's own room is restored.
- `chorusctl inputs unlabel <input>` removes the label; `chorusctl inputs labels` lists them.
  A label is kept whether or not the input is offered now.

## Where it is tested

- `crates/server/tests/alarm_stored_sources.rs`: the real binary, a stored URL alarm with
  `--upnp` off and on, each fallback reason, the endless stream stopped by the alarm's stop.
- `crates/server/tests/line_in_sharing.rs`: the real binary, sharing, the latency through a
  share and a leave, an alarm on a shared input, the streamer.
- `crates/server/tests/tv_low_latency.rs`: a TV input a second group plays leaves low-latency
  mode and returns to it, on the real binary and the real client code.
- `crates/server/tests/schedule_runtime.rs`: the runtime's rules on modelled time, the Spotify
  seam included.
- `crates/control/tests/catalog_v2.rs` and `fixtures/control/v2/`: the commands, their
  refusals and the state, byte for byte; `crates/control/tests/inputs_v2.rs`: the room model's
  rules and state-file format 6.
