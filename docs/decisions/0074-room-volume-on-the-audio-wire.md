# 0074: the room's gain and limit travel on the audio wire as room_volume, and both endpoint kinds enforce min(ramped gain, limit, own ceiling) at every frame

- Status: accepted (goal 11, 2026-10-01)
- Decided by: the goal (K81, I10, brief section 4.8; the goal 11 design envelope, "Room volume on
  the audio wire"); every default below not measured or cited is ASSUMED
- Implemented in: `docs/protocol.md` ("0x38 room volume"); `crates/protocol/src/v2/`
  (`Message::RoomVolume`, `Type::RoomVolume`); `firmware/src/protocol_v2.c`;
  `firmware/include/chorus/volume.h`, `firmware/src/volume.c`, `firmware/src/playout.c`,
  `firmware/src/session.c`, `max_volume` in `firmware/config/endpoint.conf`;
  `crates/client-linux/src/zone.rs` (`RoomGain`, `RoomVolumeInbox`), `control.rs`, `session.rs`,
  `run.rs`, `--max-volume` in `config.rs` and `deploy/endpoint/client.conf`. Held by
  `fixtures/protocol/v2/room_volume*.{hex,fields}` and `fixtures/protocol/v2/rejected/` (read by
  `crates/protocol/tests/v2_vectors.rs`, `v2_rules.rs` and `firmware/tests/test_protocol_v2.c`),
  `firmware/tests/test_volume.c` (`make firmware-check`, target `volume`) and
  `crates/client-linux/tests/room_volume.rs`

## Context

Brief section 4.8 (K81, I10): volume limits are enforced on the server AND on the endpoint, for
every path. Until goal 11 the limit lived only in the server's control plane: the Linux client
multiplied by the zone volume it read from the control plane's state (`zone.rs`, ADR 0016's
catalog), the C endpoint played at unity, and neither knew a limit. A server bug, or any path that
reached an endpoint without passing the server's clamp, could therefore make a speaker play at
full scale. Goal 11 adds rooms, quiet hours, alarms and sleep timers, each of which changes a
room's volume or limit at times nobody is watching, so the endpoint has to hold the bound itself.

## What was read

All read 2026-10-01, all chorus's own: BRIEF.md section 4.8; `docs/protocol.md` (the v2 catalog,
records, roles, decoder rules); `fixtures/README.md` and `fixtures/protocol/`;
`crates/protocol/src/v2/{catalog,codec,messages}.rs` and its tests; `firmware/src/protocol_v2.c`,
`firmware/tests/test_protocol_v2.c`, `firmware/src/playout.c`, `firmware/src/session.c`,
`firmware/src/endpoint_config.c`, `firmware/config/endpoint.conf`, `firmware/Makefile`;
`crates/client-linux/src/{run,control,config,zone,session}.rs` and
`crates/client-linux/tests/{zone_apply,line_in_source}.rs`;
`tools/conventions/check-shared-fixtures.sh`; ADRs 0041, 0058, 0063, 0066 to 0069. No external
source: no number here rests on one.

## Decision

1. **The message.** v2 type `0x38 room_volume`, server to player, in a record, a fixed 6-byte
   payload: `gain` u16 (thousandths, 0 to 1000, what to play at, 0 when muted), `limit` u16
   (thousandths, 0 to 1000, the room's effective limit: its limit less any quiet-hours cap in
   force), `ramp_ms` u16 (0 to 60000). Thousandths because the control catalog's volume is
   thousandths (`Volume`), so the server forwards a room's state without a conversion. A value out
   of range is rejected by decoder step 5, never clamped (a decoder that clamped an out-of-range
   limit would choose a limit nobody sent); the encoder refuses the same values. Each field is
   checked on its own, so a gain above the limit is a valid message.
2. **The rule, on both endpoint kinds.** At every frame written,
   `applied = min(ramped gain, last limit received, the endpoint's own ceiling)`, and on the Linux
   client also the control plane's zone gain (below). A gain above the limit plays at the limit.
   The limit applies at once, at the next frame; only the gain is ramped. A ramp is linear in
   amplitude from the gain being APPLIED when the message arrives (after the clamp, so what is
   heard never jumps up) to `gain`, over `ramp_ms` counted in frames at the stream's rate
   (`ramp_ms * rate / 1000`): monotonic by construction, and no clock is read. A new message starts
   a new ramp from wherever the old one had got to.
3. **Arithmetic.** Integer. A gain is a Q16 fraction of full amplitude (65536 is unity),
   converted from thousandths rounding down, exact at 0 and 1000; a ramp's frame k of N is
   `from + (to - from) * k / N`, truncated, kept by a remainder accumulator (no division per
   frame) that ends exactly on `to`; a sample is scaled and truncated toward zero, so it is within
   one unit in the last place of the exact product, toward silence, the zone gain's rule. Unity is
   the identity, byte for byte, so the existing playout tests, which read a frame counter back out
   of the PCM, see nothing change. On the Linux client a settled gain is a whole number of
   thousandths and goes through the existing `ZoneGain` unchanged; only a moving ramp uses Q16.
   `pcm_f32le` is scaled in `f32`, as before.
4. **Frames in == frames out.** A gain, a limit or a mute changes what a frame holds, never how
   many are written or when (the v1 mute rule, `zone.rs`): a muted room writes zeros at the same
   instants. On the C endpoint the gain is applied in `chorus_playout_fill` to exactly the audio
   frames it copies out, before the resync mute; the ramp also advances over the silence the writer
   writes (insertions, underruns, the hold before acquisition), because time passes for the ramp
   either way. On the Linux client it is applied where the zone gain was (the priming write, every
   chunk, the drain), and the ramp advances over the frames written.
5. **The ceiling.** `max_volume` in `firmware/config/endpoint.conf` (a required base key, so a
   board profile cannot set it) and `--max-volume` on the Linux client (in the package conffile's
   `CHORUS_CLIENT_ARGS`). A decimal 0 to 1 with at most three places; anything else is refused by
   name at start. **Default 1.000, ASSUMED**: a policy default, not a measurement. An owner lowers
   it for a speaker that must never play loud (a child's room, a small driver); nothing on the
   wire raises it.
6. **Before the first room_volume: the ceiling, ASSUMED.** An endpoint plays at its ceiling until
   the server sends `room_volume`. Muting until then would be the more cautious start, but today's
   server does not send the message (the integration track adds it), so every installed speaker
   would fall silent until it does; and the bound an endpoint can enforce WITHOUT the server is its
   own ceiling, which is what `max_volume` is for. The integration track's server sends
   `room_volume` before the first audio of every session (8), so the startup gain covers at most
   the frames before it arrives.
7. **Held, never reset upward.** The gain, limit and ramp last received are kept across a new
   stream and a new session (the C endpoint for its boot, the Linux client for its process, held by
   `ZoneWatch` like the zone gain). A new stream or a reconnect is not a reason to play louder.
8. **The Linux client's two paths.** It keeps the control plane's state path working: the zone
   gain from `state` (volume, 0 when muted) is a further upper bound, and a state that carries an
   `effective_limit` (the v2 state, catalog-v2 track) bounds that gain by it too. The most
   restrictive of every bound wins, whichever path it came by.

## Why both endpoint kinds enforce it

The server clamps every volume to the room's effective limit (the control plane, catalog-v2
track), and that remains the first line. The endpoint is the second: K81 and I10 say limits are
enforced on the server AND the endpoint for every path, and brief section 4.8 puts it under
security. A bug in the server's room model, a control path that reaches an endpoint around the
server's clamp, or a compromised server cannot make an endpoint play above the limit it was last
given, and nothing at all can take it above its own configured ceiling. The ESP32-S3 endpoint and
the Linux client are both endpoints, so both do it, with the same rule and the same arithmetic.

## What the server must send, and when (for the integration track)

- **At the start of every session**, after `stream_format` and before the first `audio_chunk` or
  `coded_chunk`: the room's current `gain` (its volume, 0 when muted), `limit` (its effective
  limit), `ramp_ms` 0. The scripted server in `crates/client-linux/tests/room_volume.rs` does
  exactly this.
- **Whenever the room's volume, mute or effective limit changes**, to every endpoint of the room
  (a room in a group gets its own room's values, not the group's): a volume or mute from a person
  with `ramp_ms` 0 or a short de-click ramp (the integration track's call, ASSUMED where chosen); a
  quiet-hours window starting with the new `limit` (applied at once by the endpoint; to be gentle,
  ramp the `gain` down first and lower the limit after); a window ending raises nothing, so it
  sends only the new limit.
- **Alarms and sleep timers:** an alarm's ramp up is `gain` = the alarm's volume from a muted
  start with `ramp_ms` = `ramp_s * 1000`; a sleep timer's fade is `gain` 0 with its fade time. A
  ramp longer than 60 s is sent as successive messages, each at most 60000 ms, each starting from
  where the last one got to.
- The server still clamps `gain` to `limit` itself; the endpoint's clamp is defence in depth, not
  a reason to send an unclamped gain.
- `room_volume` is a player-role message: every speaker takes it; it is independent of
  `controller_state` (which shows a controller the state, and stays 0 to 100).

## Not chosen

- **Volume on the control plane only (the status quo).** The control plane is a separate
  connection with its own failure modes, and the C endpoint does not speak it; I10 asks for the
  bound where the audio is played.
- **Scaling on the server, sending pre-scaled PCM.** The server stays the first clamp, but an
  endpoint that trusted pre-scaled PCM enforces nothing, and a FLAC or Opus stream shared by a
  group would have to be encoded once per room.
- **Ramping the limit.** A lowered limit that took a second to arrive would be a second above
  the limit. The server ramps the gain when it wants gentleness.
- **Clamping an out-of-range field in the decoder.** Every other v2 field is rejected out of
  range; a limit of 1001 is a broken sender, not a limit.
- **Floating point in the C hot path.** The playout path is integer; Q16 is exact at both ends
  and needs one 64-bit multiply per sample.
- **Muted until the first room_volume.** See decision 6: correct once the server sends it, and
  a silent house until then.

## Follow-ups

- The integration track: send `room_volume` per session as above, from the room model.
- `max_volume` is the only volume key in `endpoint.conf`; the file is within 100 bytes of the
  16 KiB the endpoint's readers hold (`chorus_endpoint_config_load_profile`, `app_main.c`), so the
  next key there needs those buffers raised first.
- Whether a de-click ramp on a person's volume change is wanted is a listening question for the
  bench, not decided here.
