# 0169: chorus-server keeps a speaker's microphone audio only while its room has `voice_enabled` and the speaker reports its gate live, in a bounded buffer in memory that nothing else can read; `voice_enabled` is a per-room command, off by default and persisted in state-file format 8, and `mic_muted` is read-only room state

- Status: accepted, 2026-10-04
- Decided by: the owner for the rule (K73: "speakers stream mic audio to chorus-server only
  while unmuted and voice is enabled"; I4: "Speaker microphones feed only the voice path, never
  a shareable source"; proposal P8, Option A, approved, with its "Mute semantics" and "Privacy
  (I4)" paragraphs); the task for the scope (the intake, the command and the two state fields;
  "an ADR if a cheap decision is made"); this record for the cheap decisions: where the gate is
  read, the buffer's bound and when it is wiped, the shape of the state fields, the state-file
  format, and what the log says.
- Recorded by: the owner's agent harness, in the pull request that adds this record.
- Implemented in: `crates/server/src/voice.rs` (`Voice`, `DropReason`, `BUFFER_SAMPLES`),
  `crates/server/src/session.rs` (`route_controller`: `mic_state`, `mic_audio`),
  `crates/server/src/conductor.rs` (`voice_controls`), `crates/server/src/control.rs`
  (`voice_room`, `speaker_mic_gate`, `RoomView::voice_enabled`); `crates/control/src/catalog.rs`
  (`Command::VoiceEnabled`), `zones.rs` (`Zone::voice_enabled`, `Zones::speaker_mic_gate`,
  `Zones::mic_muted`), `speakers.rs` (`SpeakerNow::mic_live`), `persist.rs` (format 8);
  `docs/control-plane.md` ("Voice: `voice_enabled` and `mic_muted`"); held by
  `crates/server/tests/voice_intake.rs`, the unit tests of `voice.rs`,
  `crates/control/tests/voice_v2.rs` and the vector `fixtures/control/v2/voice_enabled`.

## Context

0166 put the voice role on the wire: `mic_audio` (0x3A), `mic_state` (0x3B) and
`voice_control` (0x3C), with nothing sending or acting on them. This record is the server's
side of the first leg: what it does with a `mic_audio` frame, what it tells a voice endpoint,
and what the control plane shows and takes. The wake word, the run, the route to Home
Assistant, the integration's entities and the firmware's capture are later tasks.

## What was read

All on 2026-10-04. No GPL source and no reciprocally licensed design was opened.

- `docs/proposals/P8-voice-path.md` (Option A; "Mute semantics", "Privacy (I4)").
- `docs/decisions/0166-the-voice-role-on-the-wire.md`, `docs/protocol.md` ("The voice role").
- `docs/decisions/0150-quiet-hours-switched-off-and-on-per-room.md` (the last per-room flag:
  a command, a state field, a state-file format).
- `docs/control-plane.md` (the room object, "What survives a restart", the input labels'
  "There is no microphone role").
- `crates/server/src/{session,conductor,control,router,linein}.rs`,
  `crates/control/src/{catalog,zones,speakers,persist}.rs`.
- The owner's answers of 2026-10-04 on the voice feature: the streaming route to the
  integration is run-scoped; no voice intent targets a Soloist source; microphone hardware is
  deferred. The first is the next leg's and adds nothing here: this change serves the
  microphone's audio on no route at all.

## Decision

1. **A frame is kept only when four things hold as it arrives**: its session declared the
   voice role, its endpoint is in a room, the room has `voice_enabled`, and the session's last
   `mic_state` said `live`. Otherwise it is dropped and counted under one reason
   (`no-voice-role`, `no-room`, `voice-disabled`, `gate-muted`, checked in that order).
2. **The room's flag is read from the room model for every frame**, not from what the endpoint
   was last told. Switching a room off therefore drops its next frame even when the endpoint
   has not yet obeyed (or never obeys) the `voice_control` that tells it to stop. The cost is
   one short lock of the room model per frame, 50 a second per open microphone; the same lock
   a speaker's telemetry takes.
3. **The gate is the session's own word.** A session that has sent no `mic_state` is muted
   (0166, decision 8). Nothing in the server opens a gate; `voice_enabled` does not touch it.
4. **`voice_control` is sent by the conductor**, as `room_volume` and `controller_state` are:
   on each pass, to every voice session whose room's flag differs from what it was last told.
   `uplink` is the room's `voice_enabled`; `listening` is always false until a run exists. A
   session starts off (0166, decision 9), so one in a room with voice off is sent nothing.
5. **The buffer is per session, in memory, at most three seconds** (48000 samples, 96000
   bytes), the oldest samples falling off. ASSUMED: enough for a wake-word model's window; not
   measured, and the wake-word task settles it. It is overwritten and emptied when the room
   is switched off, when a frame is dropped, when the gate closes, when the endpoint is told
   to stop and when the session ends: a muted or disabled room holds no audio.
6. **The buffer is unreachable from everything that plays, shows or stores audio (I4).**
   `Voice` holds no reference to the router, a slot, a line-in port, the visualizer, the event
   fanout or a file, and none of them holds one to it: the session reader and the conductor
   are its only callers. `mic_audio` is its own type, so the reader's upstream path, which
   takes `audio_chunk`, is never handed it. Its two readers, `Voice::buffered` and
   `Voice::take`, exist for the wake word and have no caller yet. There is still no microphone
   source kind, input role or source spelling.
7. **The log names counts, never samples.** One line per change of what becomes of a
   session's frames, one per gate change, one per `voice_control`, one when a session that
   sent microphone audio ends; each carries `buffered_frames` and `dropped_frames`. `Voice`'s
   `Debug` prints counts only.
8. **The drop counter is in `Voice` and in those lines, not in `GET /metrics`.** The series
   names of the exporter are a contract with `docs/telemetry.md`, the rules and the dashboard;
   a new family there is a change to all three, and the task rules metrics beyond a drop
   counter out. A `chorus_voice_*` family is a follow-up.
9. **`voice_enabled` is a catalog v2 command with `zone` and `enabled`**, the shape of
   `quiet_hours_enabled`, so one switch in a home-automation system drives it. Off by default
   in every room. Persisted as `voice_enabled` (0 or 1) in `[zone]`, state-file format 8,
   required in a format 8 file as every format's own fields are; a format 1 to 7 file loads
   with every room off.
10. **The room object gains `voice_enabled` and `mic_muted`, always present**, after
    `room_eq` and before `source` and `now_playing`. Always present, unlike the fields that
    are written only when there is something to say, because "off" is the answer a consumer
    most needs to read. `mic_muted` is `true` unless a speaker present in the room has
    reported its gate live, so no microphone, an absent one and a silent one all read muted;
    which rooms have a microphone is `speakers[].roles` (`voice`). It is never persisted and
    no command sets it. The gate is kept per speaker (`SpeakerNow::mic_live`) and cleared
    when the speaker's last session ends.

## Compatibility

- Seven committed state vectors gain the two fields, and so does the MQTT room payload, which
  is the room object byte for byte (`docs/mqtt.md`). The Home Assistant client reads the
  fields it knows and ignores the rest, so it is unchanged; its entities for these two are a
  later task.
- A state file written by this build is format 8 and is refused by an older build, as every
  format step is.
- The v1 state shape is unchanged.

## Considered and not chosen

- **Gating on what the endpoint was last told (`uplink`).** One lock fewer per frame, and a
  window between the command and the conductor's pass in which a disabled room's frames are
  kept.
- **One buffer per room.** What the wake word wants in the end, perhaps; with two microphones
  in a room it needs a rule for mixing or choosing, which is the wake-word task's.
- **The voice fields written only for rooms with a microphone.** Keeps the vectors still, and
  makes "off" and "not said" the same bytes.
- **`mic_muted` as `null` for a room with no microphone.** A third state for a consumer to
  handle, for a fact `speakers[].roles` already carries.
- **A `chorus_voice_mic_frames_dropped_total` series now.** Decision 8.

## ASSUMED values

The three-second bound of the buffer. It is not a timing claim, and the wake-word task
settles it.
