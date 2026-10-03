# 0076: the schedule runtime is a pure server module that fires alarms, counts sleep timers down, follows quiet hours and runs line-in autoplay over the room model, on time it is handed, and returns the effects for the conductor to apply

- Status: accepted (goal 11, 2026-10-01)
- Decided by: the goal (brief section 15 item 2: alarms, sleep timers, quiet hours, volume ramps,
  line-in autoplay; K30, K78, K80, K81); the coordinator's goal 11 design envelope and its
  integration plan sections (c) and (d), checked against what phase A merged (ADR 0071, 0072,
  0074, 0075); the details below where those left them open, every default ASSUMED
- Implemented in: `crates/server/src/schedule_runtime.rs`, excluded in `audio-path.conf`; held
  by `crates/server/tests/schedule_runtime.rs` (26 tests) and the module's unit tests (3)
- Not yet wired: the server's slots and conductor thread are another track's; this module is
  called by nothing in `main.rs`, `session.rs` or `control.rs` yet

## Context

Phase A gave the server a room model with runtime hooks (`crates/control/src/zones.rs`, ADR
0075: `set_civil_time`, `start_ramp`, `runtime_volume`, `set_group_source`, `offer_input`,
`set_alarm_ringing`, `sleep_expired`), a schedule library that reads no clock (`crates/schedule`,
ADR 0072: TZif civil time, `due_between`, sleep plans, ramps, chimes), a `room_volume` message
with a `ramp_ms` (ADR 0074) and a line-in source role (ADR 0066). Nothing yet decides, as time
passes, that an alarm fires, a sleep timer fades, a quiet window caps a room or a line-in takes
a room. That is a state machine over civil and monotonic time, and it must be testable at any
instant of any year: so it is a pure module, separate from the thread that will run it.

## What was read

All 2026-10-01, in this repository: `crates/control/src/{zones,rooms,catalog}.rs`,
`crates/schedule/src/*.rs`, ADR 0066, 0071, 0072, 0074 ("What the server must send, and when"),
0075, `crates/protocol/src/v2/messages.rs` (`SourceOffer`, `SourceControl`, `RoomVolume`),
`audio-path.conf` and `crates/audio-path/src/scan.rs`, `docs/conventions.md`; outside it, the
coordinator's design envelope and integration plan for goal 11. No outside source is relied on
for a number: every default here is ASSUMED.

## Decision

### Shape

`Runtime::new(zone)` keeps civil time in one `chorus_schedule::Zone`. Four entry points take the
monotonic instant (ns) and, for `tick`, the civil one (UTC seconds), plus `&mut Zones`, and return
`Vec<Effect>`:

- `tick(now_mono_ns, now_utc_s, zones)`: quiet hours, ramp segments, sleep timers, alarm ends,
  autoplay holds, then alarms due since the previous tick. The caller ticks at least once a
  second and by `next_deadline_ns()`.
- `on_command_applied(command, now_mono_ns, zones)`: a person's command was applied (never one
  the runtime applied itself).
- `on_input_signal(input, signal, now_mono_ns, zones)`: a `source_offer`'s signal flag.
- `on_input_gone(input, now_mono_ns, zones)`: the input's endpoint is gone.

The runtime changes the zones itself, through the model's hooks and `Zones::apply`, so every
volume it sets is clamped by the model (never above the room's effective limit) and the state is
the truth. The serial moves on every change, so the conductor fans out exactly as after a command.

### The effects

| Effect | Meaning for the conductor |
|---|---|
| `Log(line)` | a `schedule ...` key=value line for the log |
| `SetSource { group, source }` | route the group's slot: `Chime(name)` (repeated while it is the source), `LineIn(input)`, `None` (silence), `Stream` |
| `SourceControl { input, Start or Stop }` | send `source_control` to the input's endpoint; the conductor maps the name to its `source_id` and picks the codec |
| `RoomVolume { zone, gain, limit, ramp_ms }` | send `room_volume` to every endpoint of the room |
| `Persist` | a persisted fact changed: write the state file |

They come in this order in one batch: logs, `RoomVolume` for rooms getting quieter, `SetSource`,
`SourceControl` (stops, then starts), `RoomVolume` for rooms getting louder or whose limit only
moved, `Persist`. Quieter first and louder last keeps a source change from being heard at the
wrong level: an alarm in a room playing music sends the room to silence before the chime starts,
and an alarm's end silences the chime before the room's volume comes back.

Sources, `source_control` and room volumes are **derived**: after every call the runtime compares
each formed group's source and each room's (gain, effective limit) with what it last reported,
and reports differences, whoever made them (a person's `volume` comes back as `room_volume` with
`ramp_ms` 0, ASSUMED: no de-click ramp). An input is started while a formed group plays it and its
endpoint is connected, and stopped when none does. So the conductor needs no diff of its own; it
sends each session's first `room_volume` at session start (ADR 0074) and forwards the rest. The
first call reports every room and every group once.

### Ramps are stepped

Every ramp (an alarm's rise, an alarm's end fade, a sleep fade) is a `chorus_schedule::Ramp`
sampled in segments of `STEP_MS` = 1000 ms (ASSUMED): each segment sets the room's state volume to
the value the plan reaches at the segment's end and sends `room_volume` with that gain and
`ramp_ms` = the segment's length, so the endpoint draws one straight line in amplitude. Anything
that changes mid-ramp (a limit, a quiet window, a person) therefore holds within one segment, the
state always shows where the room is going, and no message is near the wire's 60000 ms cap. An
alarm's rise is also held under the model's ramp target (`Zone::ramp`), which the model keeps
clamped: a quiet window starting mid-ramp holds the rise at the window's cap. Intermediate steps
do not ask to persist; the end of a ramp does.

### Quiet hours

Each tick converts the civil instant to the house's weekday and minute and calls
`set_civil_time`; the model pulls volumes down (a window ending raises nothing). Following ADR
0074's advice, a lowered limit goes out after the gain: the first `room_volume` carries the new
gain with the old limit and `ramp_ms` `LIMIT_PULL_MS` = 1000 (ASSUMED), and the new limit follows
on the first call at least that much later. When the gain did not fall, the limit goes out at once.
The same applies to a person's `limit`.

### Alarms

- **Polling.** `due_between(previous tick, now)` in the configured zone, half open, so DST gaps
  ring at the first valid instant and folds ring once, at the first (ADR 0072's rules). The first
  tick after start rings nothing missed while the server was down.
- **Never twice.** Each alarm's last ring instant is kept; a due instant at or before it is
  ignored. A civil clock stepping backwards is logged (`civil clock stepped back by N s`) and
  polling continues from the new reading; the instant that already rang does not ring again.
- **Late.** A fire more than `LATE_FIRE_S` = 60 s (ASSUMED) late is skipped and logged
  (`skipped late`), and counts as its ring for the never-twice rule. A one-shot alarm is disabled
  (through `alarm_set`, persisted) when it rings or is skipped. An alarm due while it is still
  ringing is logged and not refired.
- **Fire.** Each target room (a room, or a saved group's rooms) is snapshotted (group, volume,
  muted, its group's source, the other rooms of its group), the target is taken (`take`, K78) with
  the alarm's source, each room is set to volume 0 and unmuted, and the rise starts on the next
  tick from 0 to the alarm's volume clamped by `start_ramp` over `ramp_s` (0 = at once). The alarm
  is marked ringing.
- **Fallback.** A line-in that is not offered (no signal), already played by another group (one
  input feeds at most one group; **retired by ADR 0129, goal 17**: a line-in plays in any number
  of groups and an alarm plays one another group plays), a chime name that does not exist, or a
  source of `none` plays
  `FALLBACK_CHIME` = `bell` (ASSUMED) instead, logged `fallback=chime reason=...`. A line-in
  whose endpoint goes while the alarm rings falls back the same way (`reason=input-gone`): an
  alarm must still wake.
- **End.** On `duration_min`, `alarm_stop`, `alarm_delete`, or a person's command naming a target
  room: the alarm stops ringing; every room it still holds fades from its volume to 0 over
  `END_FADE_MS` = 2000 ms (ASSUMED); then the group's source is set to `none` if it is still the
  alarm's (a `source_control` stop follows for a line-in), and the rooms are restored. A room a
  person's command named is detached at once: no fade, no restore, the person's volume kept.
  `alarm_set` replacing a ringing alarm does not stop it.

"A person's command naming a room" is `volume`, `volume_step`, `mute`, `group`, `ungroup`, `join`
(the room and a target room), `take` (the target's rooms), `group_volume` and
`group_volume_step` (the group's rooms). `limit` and `quiet_hours` are not: the clamp already
honours them. Nor are `name`, `bond`, `unbond`, an endpoint's `attach`, or the schedule's own
commands. This follows the envelope's rule as written (any such command ends the alarm, so turning
a ringing alarm's room up also ends it); a gentler reading (adjusting volume keeps it ringing) is
a follow-up for the owner, not a change made here.

### Restore

A room goes back to its snapshot: its own group (`ungroup`), or the group it was in when that is
formed or saved (`join`), or, when it was in a live group that dissolved while it was away,
`join` a room of that group that is not itself still waiting to be restored (which forms a live
group again, under a new id: a live group's id is never chosen), else its own group. Then its
volume (through the clamp: a limit lowered meanwhile wins) and mute. A group made only of rooms
restored together gets back the source the snapshot recorded; a group with other rooms keeps
playing what it plays. One snapshot per room: an activity that takes a room another holds
inherits that snapshot, so the room goes back to what it was before either took it.

### Sleep timers

`sleep` starts a monotonic `chorus_schedule::SleepTimer` at the command's instant (a reissued
`sleep` restarts it; `sleep 0` cancels). A timer the model has and the runtime was not told of
starts at the next tick; one the model dropped (its group dissolved) ends. At the fade start
(`SLEEP_FADE_S` = 30 s before expiry, ADR 0072, ASSUMED there) each of the target's rooms is
captured with its volume and fades to 0 in stepped segments, ending at expiry; at expiry each
room's group gets source `none` and each volume goes back to the captured one. A person's volume
change (`volume`, `volume_step`, `group_volume`, `group_volume_step`) on a fading room cancels the
timer: the touched room keeps the person's volume, the others go back to their captured volume
over one segment (ASSUMED).

### Line-in autoplay

The runtime owns the state's `inputs`: an input is offered while its signal is present (the
model's `offer_input` contract, ADR 0066's presence rule), withdrawn when it goes. On signal
present with an enabled rule, and no group already playing the input: the target is snapshotted
and taken with source `line-in:<endpoint>/<input>`, and a `source_control` start follows; the
rooms' volumes are left as they are (ASSUMED). On signal gone the input is held
`AUTOPLAY_HOLD_MS` = 30 s (ASSUMED, on top of the endpoint's own 2 s hysteresis), then the group
goes to `none`, the input is stopped and the rooms restored; the signal returning within the hold
cancels the stop. A person's command on a target room detaches that room (it stays as the person
left it). The input's endpoint going ends the autoplay at once, with no `source_control` (there is
no session to send it on). A rule is consulted when the signal arrives: disabling it does not stop
a line-in already playing. An autoplay whose input no group plays any more (a person chose
another source, an alarm took its rooms) ends without a restore.

### Every value ASSUMED here

`STEP_MS` 1000; `END_FADE_MS` 2000; `LATE_FIRE_S` 60; `AUTOPLAY_HOLD_MS` 30000; `LIMIT_PULL_MS`
1000; `FALLBACK_CHIME` bell; a person's volume `ramp_ms` 0; tick at least once a second; a
cancelled sleep fade restored over one segment; autoplay keeps the rooms' volumes. From ADR 0072,
also ASSUMED there: the 30 s sleep fade.

## Not chosen

- **One `room_volume` per ramp with `ramp_ms` = the whole ramp.** Simple, but a cap or a person
  mid-ramp would have to interrupt a ramp the endpoint is running, the state would show the end
  value for the whole ramp, ramps over 60 s need splitting anyway, and a late-joining endpoint
  would get no point on the line.
- **Effects for every model change** (membership, mute) instead of the derived reports. The
  conductor already re-routes sessions from the zones after any change; reporting the derived
  facts (sources, inputs, room volumes) is what it cannot read off the state cheaply, and a single
  deriver means a person's change and the runtime's are reported the same way.
- **Restoring a dissolved live group under its old id.** Live ids are assigned by the model and a
  stale one could collide; forming it again by `join` is the model's own rule.

## Follow-ups (for the integration track)

- The conductor thread: read `CLOCK_MONOTONIC` and the civil clock, call `tick` at least once a
  second and at `next_deadline_ns()`, call `on_command_applied` after every applied command (the
  control plane's and the controller role's), map each `Effect` onto the slots and sessions, and
  load the zone at start (`--tz`, `$TZ`, `/etc/localtime`, else UTC, reported).
- Route `source_offer` to `on_input_signal` and a source session's end to `on_input_gone`; map an
  `InputId`'s input name to the endpoint's `source_id`.
- A chime slot that repeats the chime while it is the group's source.
- Whether turning a ringing alarm's room up should end it (the rule above follows the envelope).
