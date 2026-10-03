# 0119: a network media player is a source `player:<id>` that plays in one group, heard through a player port the audio thread takes one chunk from a tick, on threads that exist from the start; what it plays is a now-playing record on the group and on each of its rooms, written only when there is one

- Status: accepted (goal 16, 2026-10-03)
- Decided by: the goal (program section 20) inside the coordinator's goal-16 design envelope
  ("Control model (track S)", "Server plumbing (track S)"), track `chorus-g16/player-plumbing`;
  every number below not cited is ASSUMED
- Implemented in: `crates/control/src/rooms.rs` (`Source::Player`, `PlayState`, `NowPlaying` and
  its bounds), `crates/control/src/zones.rs` (the one-group rule, `set_now_playing`,
  `now_playing`, `player_group`, the two state members), `crates/server/src/playerport.rs` (new,
  on the audio path: the port), `crates/server/src/player.rs` (new: the player threads and the
  driver interface), `slots.rs` (`SlotInput::Player`, `SlotMedia::players`), `conductor.rs`
  (`player:p<i>` to its port), `control.rs` (`set_players`, `set_now_playing`, `player_group`,
  the refusal of a player the server does not run, `controller_state_of`), `config.rs`
  (`--players`), `main.rs` (the ports, the threads, the count, `USAGE`),
  `schedule_runtime.rs` (an alarm with a player source rings the bell),
  `crates/ctl/src/render.rs` and `crates/server/src/ui/chorus.js` (title and artist where rooms
  are listed); held by `crates/control/tests/now_playing_v2.rs`, `catalog_v2.rs` with
  `fixtures/control/v2/{take-player,error-take-player-busy,state-playing}`,
  `crates/server/tests/player_port.rs`, `control_thread_population.rs`,
  `crates/mqtt/tests/payloads.rs`, `crates/ctl/tests/output.rs`. The contract is
  `docs/control-plane.md`; the MQTT side is `docs/mqtt.md`

## Context

Goal 16 adds network media playback: a UPnP AV media renderer for every room, saved group and
live group. Its decode threads (the track after this one) need three things from the server
that have nothing to do with networks or codecs: a way for a group to play decoded audio, a
place in the control state to say what is playing, and threads that exist before the scheduling
report. Goal 15's adversarial check also left a gap this closes: the MQTT room topic said
nothing about what a room plays (ADR 0116, "Follow-ups").

The constraints are chorus's own. The audio thread waits on nothing but its grid and allocates
nothing per tick (ADR 0077). Every thread exists before the scheduling report and is graded
against `/proc` (`crates/server/src/main.rs`). The control catalog refuses what it does not
declare, and its committed vectors are a contract (ADR 0075).

## What was read

All 2026-10-03, all chorus's own: the goal-16 design envelope and its code survey; `CLAUDE.md`;
`docs/conventions.md`; `docs/control-plane.md`; `docs/mqtt.md`; ADRs 0071, 0075, 0077 and 0116;
`crates/control/src/{rooms,zones,catalog,persist,fanout}.rs` and its tests;
`crates/server/src/{slots,linein,conductor,control,config,main,clients,session,router,mqtt,
schedule_runtime,hostreport}.rs`; `crates/server/tests/{common/mod,control_thread_population,
stream_slots}.rs`; `crates/mqtt/src/payload.rs` and `tests/payloads.rs`;
`crates/ctl/src/{render,grammar,parse}.rs`; `crates/server/src/ui/chorus.js`;
`.config/nextest.toml`; `audio-path.conf`. One external fact: RFC 8259 section 6 (numbers;
interoperable whole numbers lie within plus or minus 2^53 - 1),
<https://www.rfc-editor.org/rfc/rfc8259#section-6>, ASSUMED from memory, not re-read today.
No GPL or LGPL source, and no media renderer's or player's source of any licence, was opened;
nothing here was written from another project.

## Decision

1. **`player:<id>` is a source.** `Source::Player(String)`, spelled `player:<id>` with `<id>`
   an identifier, in `literal`, `parse` and `SOURCE_SPELLINGS`. The catalog checks the spelling;
   which players exist is the runtime's, as with chimes and inputs. The server's are `p0` to
   `p<N-1>` of `--players N`. No catalog version moves: v2 gains a spelling of an existing
   field, the way goals 12 to 14 added fields.

2. **A player the server does not run is refused by name.** An unknown chime or a line-in that
   is not offered is NOT refused today: its group plays silence (`Conductor::input_for`), and an
   alarm falls back to the bell. For a player that is the wrong answer, because a renderer that
   took a room for a player that does not exist would report success and play nothing. So
   `ControlState::apply` refuses a `take` whose source names a player outside `p0..p<N-1>`,
   field `source`: `there is no player 'p7' on this server; its players are p0, p1`. The
   conductor still maps an unknown player to silence, for a source that arrived another way (an
   alarm's, which rings the bell with reason `player-source`: an alarm has no media to hand a
   player).

3. **One group per player.** A player is one stream of decoded audio with one position. A
   `take`, or the runtime's `set_group_source`, that would give a second formed group the same
   player is refused, field `source`, naming the group that has it and how to get it
   (`fixtures/control/v2/error-take-player-busy`). When groups re-form the player follows its
   group through `move_source` exactly as a line-in does (a live group forming around a room, a
   group dissolving into its last room). `take` has one place where a source is COPIED: rooms
   pushed out of the target's group "keep playing what they were playing". A player cannot be
   copied, so there it stays with the target and the rooms pushed out play `none`; when the same
   `take` gives the target another source, the player and its record go with the rooms pushed
   out. Keeping it with the target is the choice that leaves a renderer bound to that group
   driving what it was driving. The schedule runtime's restore after an alarm or an autoplay is
   the one caller for which the refusal is not the end: a room whose player another group took
   meanwhile is given `none` (logged `reason=player-busy`), not left on the alarm's chime.

4. **Now-playing is a record per group, set by the runtime, never persisted.**
   `NowPlaying { title, artist, album, art_url: Option<String>, duration_ms: Option<u64>,
   state: playing | paused | buffering, via: String }`, set with
   `Zones::set_now_playing(group, Option<NowPlaying>)` (and `ControlState::set_now_playing`,
   which fans the state out). It is stored beside the group's source and lives and dies with
   it: moved by `move_source`, dropped by `prune` when the group is no longer formed, and
   cleared by `set_source` the moment the group plays anything that is not a player source. A
   record for a group that does not play a player source is refused, so "cleared when the
   source stops being a player" has no window in which it can be put back. The serial moves
   only when the record changed, so a driver that says the same thing again fans nothing out.

5. **Its bounds.** The model stores `NowPlaying::bounded()`: title, artist and album have
   control characters replaced by spaces, are trimmed, cut to 256 bytes at a character boundary
   and absent when empty (ASSUMED: 256 bytes shows any title a page has room for and bounds
   what a file's tags can put into every state message); `art_url` must start `http://` or
   `https://`, hold no space or control character and be at most 2048 bytes, else it is absent
   (a cut URL names something else; a `javascript:` URL must never reach a page); `duration_ms`
   above 2^53 - 1 is absent; `via` must be an identifier or the record is refused. The text
   comes from media files and control points on the LAN, so it is treated as untrusted.

6. **Where it is emitted, and why the committed vectors did not move.** `now_playing` after
   `audio` on the `groups[]` entry, and `source` then `now_playing` last on each member room's
   `zones[]` object, ONLY while the group has a record. The room is where a consumer of one
   room reads: the MQTT room topic is the room's object cut out of the state byte for byte
   (ADR 0116), and formed groups are not published, so a record on the group alone would never
   reach MQTT. `source` rides with it on the room so that consumer knows which player without
   reading `groups[]`. A player source with no record adds nothing to the room. The record's
   seven members are always written, `null` where not known, so a reader sees one shape. This
   is goal 14's "written only when there is something to say" rule, and every state vector
   committed before this change is byte-identical; `state-playing` is new.

7. **The one existing vector that changed.** `fixtures/control/v2/error-source.json` quotes
   `SOURCE_SPELLINGS`, and a catalog whose set of sources grew cannot truthfully list the old
   set. Its detail now ends `'stream', 'none', 'chime:<name>', 'line-in:<endpoint>/<input>' or
   'player:<id>', each name an identifier`. No other committed fixture changed. The same list
   is said in `chorusctl`'s `--source` help and refusal and in `docs/chorusctl.md`, changed to
   match.

8. **The player port** (`crates/server/src/playerport.rs`, on the audio path). A ring of
   interleaved frames at the server's rate and channels behind one mutex, as a line-in's port
   is, holding one second (`PLAYER_RING_MS`, ASSUMED, the line-in ring's size: it covers a
   producer that was not scheduled; it costs no latency, since a flush is immediate and a hold
   takes effect at the next tick).
   - Producer: `room()`, `write(&[f32]) -> frames accepted` (takes what fits; nothing is ever
     dropped or overwritten; a write larger than 4096 frames is copied in several holds of the
     lock so the audio thread never waits behind a whole ring), `flush() -> frames played
     before it`, `set_paused`, `finish()`, `mark()`, `played_frames()`, `underruns()`.
   - Audio thread: `play(frames, format, out)` takes up to one chunk, oldest first, encodes it
     with `linein::encode_sample`, and pads the rest with silence. No allocation; no wait but
     the lock. A held port gives silence and takes nothing.
   - **The counter.** `mark()` is the frames written and `played_frames()` the frames taken,
     both since the last flush and both moved under the ring's lock. The ring is first in,
     first out and padding is not counted, so the frame written when `mark()` read N has gone
     out exactly when `played_frames()` exceeds N. That is the producer's position, and the
     audible boundary of a second track written straight after the first with no flush: the
     gapless property is that the audio thread never sees a boundary at all.
   - **A short ring is played, not held back.** The audio thread takes what is there and pads.
     Holding back until a whole chunk is queued would need the producer to say "this is the
     end" for the last frames ever to play; a producer that forgot would hang waiting for its
     own tail. `finish()` therefore decides only what a dry ring MEANS: after it, running dry
     is the end; before it (and once something was written since the flush) it is an underrun,
     counted per tick.
   - Samples are stored as f32, not the envelope's f64: that is what the producer hands in,
     f32 to f64 is exact at the conversion, s24 is exact in f32, and it halves memory the server
     locks.

9. **In the slots.** `SlotInput::Player(u8)`, `SlotMedia::players`. A port is taken from once
   per tick however many slots name it, under the grid guard, just before the broadcast. There
   is no latency plan: decoded media is not live, its chunks go out on the grid like the
   configured stream's, and the group's tier latency applies as for `stream`.

10. **Threads behind a flag, bodies behind a trait.** `--players <N>` (default 0; at most 16,
    ASSUMED: half the slot ceiling; needs `--slots` of at least 1 and is refused by name
    otherwise). N ports and N threads `player-<i>` are created with the rest before the
    scheduling report and counted in it: `6 + 2N + M + 1 + P`. Without the flag there is no
    port, no thread and no line. What a thread does is its `PlayerDriver`
    (`fn run(&mut self, port: Arc<PlayerPort>, keep: &AtomicBool)`), handed to
    `player::spawn`; this change ships `IdleDriver`, and the renderer hands its decode loops to
    the same call without touching the population.

11. **Controllers.** `controller_state_of` says `paused` while the room's group's record says
    paused, and `playing` otherwise. Buffering is shown as playing: the wire has three words
    (`docs/protocol.md`, 0x33) and "stopped" would be wrong for a room that is about to play.

## The tests

- `crates/server/tests/player_port.rs`: the server's pieces assembled in process as `main.rs`
  wires them, a real v2 client session on loopback, and the test as the producer. One run:
  start from a chunk boundary with no gap, a hold (silence, the client holding exactly
  `played_frames()`, nothing lost), an underrun (silence, counted, nothing repeated), a flush in
  mid-play (the client receives exactly the count `flush()` returned and nothing after), a flush
  while held (never heard), two tracks back to back (one unbroken run, the second's first frame
  at index `mark()`), and an end in 17 frames of a chunk, padded. Then the whole run compared
  frame for frame. No assertion is about how long anything took; the binary is in nextest's
  `wall-clock` group because it paces real audio.
- `control_thread_population.rs::the_players_are_declared_threads_with_their_flag_and_none_without`
  grades both shapes against `/proc` on the real binary.
- `crates/mqtt/tests/payloads.rs` and a test in `crates/server/src/mqtt.rs` hold the room
  payload to carrying `source` and `now_playing`. No line of the publisher or of `crates/mqtt`
  outside its tests changed.

## Not chosen

- **Reusing the line-in port and its latency plan.** `linein::Port::write_pcm` drops a chunk
  that does not fit, which is right for a live input and wrong for decoded media, and its
  player starts a latency-growth plan (ADR 0071) meant for a signal that is already late. A
  decoded stream has no capture instant to be late against. A sibling type keeps
  `play_line_in` untouched.
- **A thread per stream, created when a stream starts.** The thread contract forbids it: a
  thread created after the scheduling report inherits a policy nobody graded.
- **Persisting now-playing, or the player source.** Both are facts about now; a restarted
  server has no stream to resume and would show a title over silence.
- **Now-playing on the group only.** It would not reach MQTT (above).
- **Always writing `now_playing: null`.** It moves every committed state vector for a fact
  that is absent.
- **A lock-free ring.** No `unsafe` is allowed here, and the line-in port's measured-adequate
  shape is the precedent; a wait-free ring is a follow-up if a tick measurement asks.
- **Holding a partial chunk back** (above, decision 8).
- **Letting two groups share a player.** One position, two listeners, and a pause in one room
  stopping the other.

## Deviations from the envelope

- The ring holds f32, not f64 (decision 8).
- `flush()` returns the frames played before it, and `finish()` exists; the envelope names
  neither. Both came from writing the test: without the first a producer cannot say where a
  stop landed, and without the second the end of media counts as underruns.
- The task asked to mirror how an unknown chime or line-in is refused. They are not refused
  (decision 2); a player is, in `ControlState::apply`.

## ASSUMED

The ring (1 s), the write slice (4096 frames), the player ceiling (16), the text bound (256
bytes), the URL bound (2048 bytes), the writer's poll (5 ms). None is measured and none is a
timing claim about audio. The mutex is "microseconds against a 20 ms tick" by the line-in
port's own unmeasured reasoning; ADR 0077's follow-up (measure the tick at S = 32) covers it.

## Follow-ups

- The renderer (goal 16, track R): drivers that fetch, decode and write; `set_now_playing` from
  DIDL-Lite, tags or ICY; what a renderer does when its group's source is taken away (an alarm,
  another `take`): it learns it from `ControlState::player_group`.
- An alarm that restores a room's player source restores no record: the record was cleared when
  the chime took the group, and comes back when the player's driver says it again.
- `docs/protocol.md` does not yet say when `controller_state` says paused; the rule is in
  `docs/control-plane.md`.
