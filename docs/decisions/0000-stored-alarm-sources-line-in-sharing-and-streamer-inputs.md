# 0000: a stored source is an alarm's only, played by a held player session the conductor starts and answers for; a line-in plays in any number of groups on one port with one latency, the largest any listener needs; a shared TV input leaves low-latency mode; a refused line-in start gets no wire message; and a streamer is a labelled line-in that plays into its room

- Status: accepted (goal 17, 2026-10-03)
- Decided by: the goal (program section 21, item 3; K80, K94, K60) inside the coordinator's
  goal-17 design envelope (section 3, "Inputs (L)"), track `chorus-g17/inputs`; every number
  below not cited is ASSUMED
- Implemented in: `crates/control/src/rooms.rs`, `catalog.rs`, `zones.rs`, `persist.rs`;
  `crates/server/src/schedule_runtime.rs`, `conductor.rs`, `playersessions.rs`, `linein.rs`,
  `main.rs`, `upnp.rs` (three small edits: the sessions are handed in, not made there);
  `crates/ctl/src/grammar.rs`, `parse.rs`, `render.rs`; `fixtures/control/v2/` (thirteen new
  vectors); `crates/server/tests/alarm_stored_sources.rs`, `line_in_sharing.rs`,
  `common/line_in.rs`, `schedule_runtime.rs`, `chorusctl.rs`; `.config/nextest.toml`;
  `docs/inputs.md`, `docs/control-plane.md`, `docs/chorusctl.md`
- Builds on: ADR 0066 (the source role), ADR 0071 (latency growth), ADR 0075 (catalog v2),
  ADR 0076 and 0079 (the schedule runtime and its wiring), ADR 0119 (player sources and
  now-playing), ADR 0124 (the engine; no command plays a URL)

## Context

K80 asks for four alarm sources: a chime, a line-in, a stored stream URL and a Spotify
playlist. Goals 11 and 16 built the first two and the machinery the third needs (a player
pool, a fetcher under a policy, player sessions), but nothing connected an alarm to a player:
an alarm with a `player:` source rang the bell, the sessions lived inside the UPnP renderers
and their reports were only taken with `--upnp`. Goal 17 also asks for a line-in shared to any
group (ADR 0066's follow-up) and "a certified streamer on a line-in as an input".

Four things constrained the answers. Brief section 4.8: "No arbitrary URL fetch except the
input paths the decisions name (UPnP renders, HA's media and TTS URLs from HA's own address,
stored alarm stream URLs)". ADR 0076: the schedule runtime is pure (no clock, no socket, no
thread) and everything it cannot do is an effect. The thread contract (the header of
`crates/server/src/main.rs`; ADR 0119): no thread is created after the scheduling report,
none per stream. And every committed vector
stays the bytes it was unless a decision here retires it.

## What was read

All in this repository at `7361ae4`, 2026-10-03. No GPL or LGPL source, no reciprocally
licensed hardware design, no Soloist file and no external web page was opened.

- The rules: `CLAUDE.md`, `docs/conventions.md`, the goal's agent rules and design envelope
  (`/cache/tmp/chorus-g17/agent-rules.md`, `design.md` sections 0, 1, 2.5 and 3) and the
  coordinator's code survey (`research/survey.md` sections A, C, D, F, H, I).
- The program: `.claude/goals/2026-09-chorus.md` section 4.8, section 21, the K80 and K94 rows.
- Decisions, in part and said so: ADR 0066 (its alternatives and follow-ups), 0071 (its
  headings and the paragraph on a target set while a transition runs; the plan's own doc
  comments in `crates/sync/src/latency_grow.rs`), 0075 (headings), 0076 (fire, fallback,
  end), 0079 (item 6), 0119 (headings and its lines about alarms and the restore), 0124 (its
  header and context), 0020 (its lines on the busy-worker refusal). What these records say
  beyond those parts is taken from the coordinator's survey, not from a reading of them.
- Code: `crates/control/src/rooms.rs`, `catalog.rs`, `zones.rs`, `persist.rs`;
  `crates/control/tests/catalog_v2.rs`, `vectors/mod.rs`; `crates/server/src/schedule_runtime.rs`,
  `conductor.rs`, `playersessions.rs` (whole); `linein.rs`, `slots.rs`, `control.rs`,
  `mediaplayer.rs`, `tvrelay.rs`, `main.rs`, `upnp.rs` (the parts about ports, the latency
  plan, the commit paths, the pool and its reports, the relay's header, the thread list, the
  sessions and the manager); `crates/ctl/src/grammar.rs`, `parse.rs`, `render.rs` and its
  tests; `crates/server/tests/alarms_sleep_autoplay.rs`, `schedule_runtime.rs`,
  `upnp_control_point.rs` (helpers), `media_player.rs` (its media server), `common/mod.rs`;
  `tools/conventions/check-shared-fixtures.sh` (`fixtures/control` is Rust-only);
  `.config/nextest.toml`; `fixtures/README.md`.

## Decisions

### 1. A stored source is a catalog record, and `stored:<id>` is an alarm's source only

`source_store {id, kind, value, name}` and `source_forget {id}` keep at most 32 named sources
(the bound every definition list has, `MAX_DEFINITIONS`). `kind` is `url` (an `http://` or
`https://` URL with a host, no control character, space, quote or backslash, at most 2048
bytes: ASSUMED, the art URL's bound) or `spotify`
(`spotify:<track|album|playlist|episode>:<id>`, the id 1 to 64 ASCII letters and digits:
both the shape and the bound are the design envelope's and ASSUMED; no Spotify document was
read for this record). The shape is checked when it is stored;
the fetch policy is asked when it is played, because whether a name resolves to a refused
address is a fact about that morning.

The source spelling is `stored:<id>`. It parses as a `Source` so an alarm stays one record
with one `source` field, but it is never a group's source: a `take` naming it is refused by
name in the decoder (field `source`), `Zones::take` and `Zones::set_group_source` refuse it as
well, and the conductor maps it to silence should it ever get that far. So brief 4.8 holds by
construction: storing plays nothing, and the only reader of a stored URL is a ringing alarm.
`SOURCE_SPELLINGS` (what a `take`'s refusal quotes) is unchanged, so `error-source.json` did
not move; an alarm's refusal quotes `ALARM_SOURCE_SPELLINGS`, which adds `'stored:<id>'`.

An `alarm_set` naming a stored source that does not exist is refused, and so is a
`source_forget` of one an alarm plays, so an alarm never points at nothing through the
catalog; a hand-edited state file that makes one is refused at load, and the runtime still
has a fallback reason for it (`not-stored`).

The state carries `stored_sources` and `input_labels` after `inputs` and before `speakers`,
each written only when non-empty: no committed state vector moved. Stored values are in the
state as they are; `docs/inputs.md` says not to store a URL with a credential in it.

State-file format 6 adds `[stored-source <id>]` (`kind`, `value`, `name`) and
`[input-label <endpoint>/<input>]` (`name`, `role`). Formats 1 to 5 load unchanged.

### 2. An alarm's stored URL plays through a held player session; the runtime stays pure

The runtime gained a plan with three outcomes (a source the room model plays, the fallback
chime with a reason, a stored source something outside must start), three effects and two
entry points:

- `Effect::PlayStored { alarm, target, stored, url, name }`: the alarm fired with its group on
  `none` (silence; the ramp runs meanwhile) and asks for the play.
- `Runtime::on_alarm_source_started(alarm, source, now, zones) -> (bool, Vec<Effect>)`: the
  group plays `source` from now (`player:p<i>`). `false` when the alarm no longer rings, is
  ending, was not waiting, or the model refused; the caller then gives the player back.
- `Runtime::on_alarm_source_failed(alarm, reason, detail, now, zones) -> Vec<Effect>`: the
  fallback chime, with the reason, at once or minutes later.
- `Effect::StopStored { alarm }`: the alarm ended; stop what was started for it.
- `Effect::PlaySpotify { .. }` and `Runtime::set_soloist_alarms(bool)`: decision 3.

The conductor carries them out. `PlayerSessions` gained a second kind of session, HELD
(`play_held`): the caller, not a `take` command, makes the group play the player, and when the
track ends or fails the session touches no group, gives the player back, records the end
(`take_ended`) and wakes the conductor. Two facts forced that shape. A session's own `take`
goes through `ControlState::apply`, which the runtime is told of as a PERSON's command, and a
person's command naming an alarm's room ends the alarm: the alarm's own source change would
have stopped it. And a session's own end sets its group to `none` the same way, which would
have ended the alarm at the very moment it must fall back. With a held session every change
to the alarm's group is the runtime's own.

The sessions are one table now, made in `main.rs` and handed to both callers
(`Upnp::new` takes it as a parameter). The players have one report stream: with `--upnp` the
renderers' manager takes it as before (a report for a player no renderer holds already went
to the sessions); without, the conductor takes it with `try_recv` at the top of each pass,
and looks every 50 ms (`RETRY`) instead of every 200 ms while a held session is live. No
thread was added and none is created per alarm. `reconcile` was made safe for two callers
(the one that takes the session unloads).

An alarm's play carries an epoch above every renderer's (`0xffff_ffff` in the half a
renderer's base occupies), so a report left from a renderer's play of the same player is
older than the alarm's session, and no renderer takes an alarm's report for its own.

Reasons, each a line `schedule alarm=<id> fallback=chime reason=<r> wanted=stored:<sid>
plays=chime:bell detail="<words>"`: `no-players`, `no-free-player`, `not-started`,
`url-refused` (the engine's failure starts `refused:`), `stream-failed` (any other failure),
`stream-ended`. The now-playing record carries the stored source's name as its title and
`via` `alarm`.

### 3. The Spotify alarm source: validated and stored here, played by the Soloist track

A stored source of kind `spotify` rings the fallback chime with reason `soloist-off` unless
`Runtime::set_soloist_alarms(true)` was called, which nothing calls yet: the design envelope
(from P7) ships it switched off. Switched on, the alarm fires exactly as for a URL and asks with `Effect::PlaySpotify`;
whoever carries it out answers with the same two entry points (the receiver's source for
`started`; `soloist-unavailable`, `soloist-logged-out`, `soloist-expired` or
`soloist-timeout` for `failed`). Today the conductor answers `soloist-unavailable` at once.
The Soloist server track fills the conductor's arm and sets the switch from its flag; the
runtime, the catalog and the vectors need no change.

### 4. A line-in plays in any number of groups

The room model never had an ownership rule for a line-in (`take` already copies it to the
rooms it pushes out) and the audio thread already plays a port once a tick for every slot
that cuts it. What forbade sharing was four places written on "one input, one group", and
each is gone:

- The alarm's `input-busy` fallback: an alarm whose line-in another group plays now plays it.
- Autoplay's refusal: an input another group plays is autoplayed into its target too. Only a
  target that already plays it exactly as the take would leave it is left alone (nothing to
  hold, nothing to restore). An autoplay is now reaped when ITS group stops playing the
  input, not when no group does.
- The port sizing prose: S ports are still enough, for a different reason. A port is one
  input's, and every group playing an input holds a slot, so at most S inputs play at once.
- **One latency target per port.** The conductor computed a target per slot and sent it per
  port, so two groups on one input would have rewritten it every pass. It is now computed
  once per port: the largest any listening group needs (180 ms wired, the wireless tier's
  for a group with a wireless room), and L_local (30 ms) only while the input's sole
  listener is the source endpoint's own room alone in its group. The plan (ADR 0071) moves
  to it without a glitch; a target asked for while a transition runs waits for it to end,
  as before. A listener joining or leaving is one log line
  (`line-in latency port= listeners= groups= target_ms=`).

Not chosen: a plan per group on one port (per-group latency). The port's resampler and plan
are one stream of frames; two plans over it would be two resamplers and two rings per input,
allocated for the worst case, to let the source's own room stay 150 ms earlier than a room
the listener cannot hear at the same time. If a house wants that, it is its own decision.

### 5. A TV input a second group plays leaves low-latency mode while it is shared

In low-latency mode a TV input's hub sends datagrams to the relay INSTEAD of its upstream
chunks (the header of `crates/server/src/tvrelay.rs`: "The hub switches its upstream to
datagrams at its accept"), so there is nothing in the port for a second group to play. The choices were to refuse the second group,
to keep the TV's room low-latency and leave the second group silent, or to put every group
on the slot path. The third is the only one in which sharing works at all without a wire
change, and it is what a group of two rooms already does (`reason=grouped`): the conductor
says `tv-path mode=slot reason=shared` when more than one group plays the input, the relay
ends its play, the hub goes back to its upstream, every group hears the slot path in sync at
the shared latency, and the TV's room loses lip sync for as long as it is shared. When one
group is left the TV's room returns to low-latency mode by itself. `docs/inputs.md` says the
price in a person's words. Not chosen: a hub that sends both (a wire and firmware change, and
two encodes on the hub); a follow-up if a house wants TV sound elsewhere with lip sync kept.

### 6. A refused line-in start gets no wire message (ADR 0066's follow-up)

No. A start is refused in exactly one case today, a `stream_format` that is not the
server's, and the endpoint is already told by the `source_control` stop that follows. A
message carrying the reason would give the endpoint nothing it can act on: its format is
what its hardware captures, it has no person to show a reason to (ASSUMED: no endpoint
state shows one today; the controls record was not read for this), and it does not retry
with another format. The people who can act
are the ones reading the server's log (`line-in refused input= reason=format-mismatch
detail="offered ... this server streams ..."`), which has both formats. The place a refusal
should become visible to a person is the state (the input listed as offered and refused),
and that is a follow-up, below. Adding a v2 wire message, its fixtures for two
implementations and a firmware handler to say what a stop already says is not worth its
weight. Revisit when a format conversion exists (the refusal then becomes rare) or when an
endpoint can do something about it.

### 7. A streamer is a labelled line-in

`input_label {input, name, role}` with `role` `line-in` or `streamer`, at most 32, persisted,
in the state as `input_labels`. An empty name with the role `line-in` removes the label (no
second command for it). A label is kept whether or not the input is offered.

A `streamer` input:

- autoplays into its endpoint's own room when its signal appears, with no rule: the runtime
  uses goal 11's autoplay with that room as the target (the same hold, the same restore). An
  `autoplay` rule for the input, enabled or disabled, is the person's word and wins. An
  endpoint in no room is logged and not played. The label takes effect at the next signal.
- shows its label: the room model writes a now-playing record itself (title the label's
  name, `state` `playing`, `via` `streamer`) on every group playing the input, and removes
  it with the source. `Zones::set_now_playing` still accepts a record only for a player
  source (`Source::takes_now_playing`, the one place the Soloist track widens).
- is shared like any line-in (decision 4).

Not chosen: a new `SourceKind` on the audio wire. What is wired to a jack is a fact a person
knows and the endpoint does not; it belongs in the catalog, not in the endpoint's offer.
Not chosen: a microphone role (brief 4.8).

### 8. chorusctl

`inputs labels`, `inputs label <input> <role> <name>`, `inputs unlabel <input>`, and a
seventh noun: `sources list`, `sources store <id> <kind> <name> <value>`, `sources forget
<id>`. Each sent message is held to its vector (`crates/ctl/tests/commands.rs`).
`inputs list --json` is unchanged (the offered inputs), so labels have a verb of their own.
There is deliberately no verb that plays a stored source.

## What changed in committed tests and fixtures

- No committed vector's bytes moved. Thirteen vectors were added under `fixtures/control/v2/`.
- `STATE_FORMAT` is 6: four tests that asserted `format = 5` and one that used `format = 6`
  as the format nobody reads were moved up one.
- `line_in_autoplay_feeds_at_most_one_group` (the runtime's pure tests) was retired by
  decision 4 and replaced by `line_in_autoplay_shares_an_input_another_group_already_plays`
  and `line_in_autoplay_leaves_a_target_that_already_plays_the_input`.
- The scripted line-in endpoint and the listeners moved, unchanged but for visibility and a
  lateness counter, from `alarms_sleep_autoplay.rs` to `tests/common/line_in.rs`, so the new
  `line_in_sharing.rs` is its own binary and the existing file's load is what it was.
- The log line `line-in latency port= slot= group= target_ms=` became
  `line-in latency port= listeners= groups= target_ms=`.

## The tests' wall clock

`alarm_stored_sources.rs` (eight tests, about 6 s each, side by side: about 16 s as a
binary) grades values and orders only: a starved machine pads a player port with silence,
which the comparison skips, so it is not in the wall-clock group. `line_in_sharing.rs` (four
tests, about 14 s side by side): three grade byte equality between groups, which holds
whatever the machine does; the fourth grades continuity through a share and a leave, runs in
the wall-clock group one at a time (about 14 s on that chain), and refuses by name a run in
which its own scripted source woke more than 15 ms late, making it again at most three
times (the analogue, for a source that must keep real time, of ADR 0020's retry past a busy
worker). Nothing here is timing evidence.

## Follow-ups

- The Soloist server track: fill `Effect::PlaySpotify`'s arm, call `set_soloist_alarms`,
  widen `Source::takes_now_playing`.
- A refused input shown in the state (decision 6), and a line-in format conversion.
- A retry for `no-free-player` when the busy player is one the alarm itself displaced
  (`docs/inputs.md`, "Known limit").
- Snooze as a control command: the schedule library has the arithmetic, the catalog has no
  command, so "snooze behaves as for the other sources" is true only vacuously today.
- A streamer label applied to an input that already has signal takes effect at the next
  signal; applying it at once is a small follow-up if the owner wants it.
- Hardware: nothing here was heard. The latency a shared input plays at on real endpoints is
  the stamp offset plus the endpoints' own playout latency (ADR 0079's follow-up).
