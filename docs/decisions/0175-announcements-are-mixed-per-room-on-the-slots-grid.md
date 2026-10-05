# 0175: an announcement is mixed over its rooms in a stream of its own on the slots' grid, so one room of a group ducks alone; the music is fully ducked before the clip's first frame and back within the restore ramp and one chunk of its last, and the state says how each announcement ended

- Status: accepted, 2026-10-05
- Decided by: the task for the scope ("the `announce` command keeps its shape and its origin
  checks but plays through the ducking mixer", for one room, for a group and for one room
  inside a playing group; "the command reports when the clip finished"; "one ADR (where the
  per-room mix happens and the timing bound)"); the owner for the Soloist rule (2026-10-04,
  on proposal P7's and P8's open input: "Leave as built (Recommended)": "announcements pause a
  Soloist source rather than duck it"); K31 ("announcements/ducking (TTS/notification over
  playback, duck and restore, HA-driven)"); 0173 for the arithmetic and its defaults; this
  record for the cheap decisions: where the mix is cut, how a room is put on it, what the room
  model shows, what ends an announcement, how its end is reported, and the bounds.
- Recorded by: the owner's agent harness, in the pull request that adds this record.
- Implemented in: `crates/server/src/mixer.rs` (`Mix`, `MixCommand`, `MixPort`),
  `crates/server/src/slots.rs` (`SlotCommand::Mix`, `SlotMedia::mixes`, the mix step),
  `crates/server/src/router.rs` (`slotted_with_mixes`, `mix_route`),
  `crates/server/src/playerport.rs` (`feed`), `crates/server/src/playersessions.rs`
  (`play_over`), `crates/server/src/announce.rs` (`Announcer::announce`, `routes`, `direct`,
  `settle`, `Mixes`), `crates/server/src/conductor.rs` (the routing and `direct` in `pass`),
  `crates/server/src/control.rs` (`announce_over_begin`, `announce_over_plan`,
  `announce_watch`, `set_announcements`), `crates/control/src/zones.rs`
  (`announce_over_begin`, `announce_over_rooms`, `announce_source`, `announce_watch`,
  `Announcement`, `AnnouncementState`); `docs/control-plane.md` ("Announcements");
  `audio-path.conf`; held by `crates/server/tests/announce.rs`,
  `crates/control/tests/announce_v2.rs` and the unit tests of `mixer.rs` and `announce.rs`;
  measured in `docs/measurements/2026-10-05-announcement-duck-timing.md`.

## Context

0136 gave chorus an `announce` command that makes the clip the SOURCE of the target's group:
the music stops, the whole group hears the clip, and the group goes back to what it played.
0173 added the arithmetic of a duck, mix and restore as a pure core with nothing calling it,
and decided that the mix runs on the server before a room's stream is encoded. This record is
the wiring: which stream is mixed, how a room comes to hear it, and what "per room" costs.

A group plays one stream: a stream slot, cut once a tick on the one audio thread and fanned
out to every session of the group (0064 and the slots' record). A room is not a stream. So an
announcement in one room of a playing group cannot be a change to the group's slot, and it
cannot be a change of the group's source either, because the other rooms keep the music.

## What was read

All on 2026-10-05, in this repository: `crates/dsp/src/duck.rs`,
`docs/decisions/0173-the-announcement-mixer.md`,
`docs/decisions/0136-a-server-identity-and-an-announce-command.md`,
`crates/server/src/{announce,slots,router,stream,conductor,playerport,playersessions,mediaplayer,control}.rs`,
`crates/control/src/zones.rs`, `docs/control-plane.md` ("Announcements"),
`docs/proposals/P7-spotify-soloist.md` ("announcements pause rather than duck"),
`docs/soloist.md`, `audio-path.conf`. No source outside this repository was opened.

## Decision

1. **A mix is one more stream on the slots' grid.** After the stream slots and the silent
   fanout the router carries announcement mixes. At every tick, under the grid guard and
   after the slots' inputs are settled, the audio thread cuts each mix from its BASE slot's
   chunk of that tick: while the mix's duck is idle it broadcasts the base slot's own frame
   (the same bytes, not a re-encoding), and otherwise it decodes that chunk, runs it through
   `chorus_dsp::duck::Duck` with the clip's next frames, and encodes the result under the
   same sequence and timestamp. A base of no slot (a group that plays nothing) is silence.
2. **A room hears a mix by being routed to it.** The conductor routes the PLAYER sessions of
   the rooms an announcement plays in to its mix, from the command to the end of the restore,
   and every other session where it was. Because chunk `k` of a mix and chunk `k` of its base
   slot carry one sequence and one timestamp, and are the same bytes while the duck is idle,
   a room moved onto a mix before its duck starts and off it after its restore hears neither
   move, and a ducked room stays on the timeline of the rooms beside it that are not.
3. **The duck is told to start only after the sessions are on the mix.** A pass moves the
   sessions and then sends `Start` (`Announcer::direct`), so the first frame of the ramp is
   heard by the rooms it is for. The clip waits in its player's port meanwhile; no frame of it
   is taken before the duck is full (0173 decision 5).
4. **No group's source changes.** The clip plays through a held player session that no group
   plays (`PlayerSessions::play_over`): it writes no now-playing record and no change of a
   group's source gives its player away. What the state shows of an announcement is decision
   8's list.
5. **A room target is that room alone.** This changes 0136, where a room in a multi-room
   group took the whole group with it: a room target ducks and mixes that room, a formed or
   saved group target every room of it (a saved group that is not active is taken first, K78,
   as before), and `volume` is set on the rooms that hear the clip and no others.
6. **The clip's end is the port's to say, on the frame it happens.** The audio thread takes
   the clip's frames from its player port under one hold of the port's lock
   (`PlayerPort::feed`) and, when the producer has said it finished and the frames handed in
   are the last, tells the duck so in the same call: the restore begins on the frame after
   the clip's last (0173 decision 6). It believes the port only for a clip it has played
   frames of since `Start`, so a port still marked finished from the clip before says
   nothing about this one. The conductor's `Finish` and `Cancel` are the backstop: a clip
   that failed, was cut at the bound or was displaced never has a last frame to see.
7. **Two mixes a player.** `2 x --players` mixes are made at start: one for the clip a player
   is playing and one for the mix whose music is still coming back when that player's next
   clip starts in other rooms. An announcement that finds none free is refused, field `t`,
   `no-free-mix`. A later announcement in rooms an earlier one is still playing in (or still
   restoring), in the same group, takes over that one's mix and player: the duck stays where
   it is, and the earlier one's rooms keep hearing the mix, because going back to the music at
   full level in the middle would be a jump.
8. **The state says how each announcement ended.** The command's answer is the state with one
   more member, `announcement`, the announcement's number. The state lists `announcements`:
   each with its `id`, `target`, `rooms` and `state`, `playing` until its clip is over and then
   `finished` (the clip played to its end), `failed` (it could not be fetched or decoded, or
   was cut at the 10 minute bound; `reason` has the words) or `displaced` (`reason` says by
   what). Every playing one is listed, and the last eight that are over. The list is written
   only while there is one to name, is never persisted and is not in the v1 shape. So a
   caller waits on the state stream it already reads, and no control worker is held for the
   length of a clip.
9. **What displaces a mixed announcement:** an alarm that rings in one of its rooms (an alarm
   is not left ducked: the mix is called off, and what its rooms hear is back at full level
   within the restore ramp), its rooms leaving the group it started in (a room that leaves
   while others stay only stops hearing it), its group taking a Spotify receiver, and a later
   announcement in the same rooms. A person who gives the group ANOTHER source does not end
   it: the base slot now plays that, and the clip goes on over it.
10. **A Spotify receiver is paused, not ducked.** An announcement whose target's group plays
    a Soloist receiver goes the way of 0136 unchanged: the clip is the group's source, the
    receiver's manager pauses the receiver, and the group plays `none` afterwards. A Soloist
    slot is never the music of a mix either: a mix whose base plays one mixes the clip over
    silence. A server without stream slots has no mix and interrupts as well.
11. **The timing bounds**, in frames of the stream at its rate, for a clip whose end is in the
    port when its last frame is taken (every clip the media player ends by itself):
    - **From the clip's first frame to the full duck: zero.** The music is at the duck gain on
      every frame that carries a clip frame; the full duck comes at least one frame before.
    - **From the clip's last frame to the full restore: at most the restore ramp and one
      chunk** (24 000 + 960 frames at the defaults and 48 kHz, 520 ms). The ramp itself is
      exactly `R` frames from the frame after the clip's last; the chunk is the allowance
      for a producer that says it finished only after its last frame went out.
    - From the first frame of the duck to the full duck is exactly `D` frames (9600, 200 ms).
    How long the command takes to reach the first frame of the duck is one conductor pass, and
    how long the clip takes to arrive is its fetch: neither is bounded here, and neither is a
    frame count.

## Considered and not chosen

- **Mixing into the group's own slot.** One mixer and no routing, but every room of the group
  is ducked, which is what the task rules out.
- **A stream slot for the mix.** The slots are a declared number of GROUPS that can play at
  once (`--slots`); an announcement that took one would be refused in a full house, or would
  silence a group to make room.
- **A second stream to the endpoint, mixed there.** 0173's "considered and not chosen".
- **Holding the command until the clip is over.** The answer would be the completion, but a
  control worker would be held for up to ten minutes a clip, out of a pool of eight by
  default.
- **An event stream for announcements.** One more subscriber protocol for what the state
  stream carries in one member.
- **Moving the announced room into a group of its own.** The room model would say the room
  left its group, every controller would redraw it, and the music under the clip would need
  a second slot playing the first one's source in step.

## ASSUMED values

- Eight ended announcements stay listed (`ENDED_KEPT`): more than a house announces at once.
- Two mixes a player.
- A mix seen restored under a clip that is still playing is started again after 500 ms
  (`RESTART_AFTER`). It can only happen when a clip is replaced in the last moments of the
  one before; not measured.
- The audio thread holds a player port's lock for one chunk's mix (microseconds against a
  20 ms tick, as `PlayerPort::play` assumes; not measured).
- 0173's own: the 200 ms duck ramp, and that -20 dB and 500 ms sound right in a room.
