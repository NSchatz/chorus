# 0000: chorus-server hears the wake word on the event writer's thread and publishes it as a `voice_wake` event with no audio and no run; a `voice_start` command opens a run and answers with its identifier, and the run's microphone audio is served on one route to that identifier, once, from the one address the server was started with, for at most the run's time limit; a pinned pairing secret is the stronger alternative and stays the owner's open call

- Status: accepted, 2026-10-05
- Decided by: the owner for the protection of the route (2026-10-04, on proposal P8's open
  input, "How is the mic streaming route protected on the otherwise unauthenticated control
  plane": "Run-scoped route (Recommended)": audio only for an active run, to a per-run
  identifier returned to the integration, only from the integration's registered address, with
  a hard time limit; "the pinned pairing secret can be added later without changing the
  satellite"), for the rule (K73, I4, K40: "chorus adds no auth of its own") and for the shape
  (proposal P8, Option A, approved); the task for the scope (the wake event, the start and stop
  commands, the route, the models in the state); this record for the cheap decisions: where
  the wake word is heard, how the integration's address is registered, the shape of the
  messages and of the route, which microphone a run reads, what ends a run, and the bounds.
- Recorded by: the owner's agent harness, in the pull request that adds this record.
- Implemented in: `crates/server/src/voice.rs` (`Voice::pass`, `start`, `stop`, `claim`,
  `read`, `reader_gone`, `listening`), `crates/server/src/events.rs` (`Feed::Mic`, the pass),
  `crates/server/src/control.rs` (`voice_through`, `voice_start`, `voice_pass`,
  `serve_voice_audio`, `GET /api/voice-events`), `crates/server/src/conductor.rs`
  (`voice_controls`: `listening`), `crates/server/src/config.rs` (`--voice-integration`,
  `--voice-run-limit-ms`); `crates/control/src/catalog.rs` (`Command::VoiceStart`,
  `Command::VoiceStop`, `VoiceWake`, `VoiceRun`, `WakeWord`), `zones.rs`
  (`voice_start_check`, `wake_words`); `docs/control-plane.md` ("Voice: the wake word, the run
  and its audio"); held by `crates/server/tests/voice_run.rs`, the unit tests of `voice.rs`
  and the vectors `fixtures/control/v2/voice_start`, `voice_stop`, `voice_wake`, `voice_run`,
  `error-voice-start-disabled`, `error-voice-start-muted` and `state-voice`.

## Context

0169 left the server keeping a voice room's microphone audio in a bounded buffer that nothing
could read. This record is the next leg of proposal P8, Option A: the server hears the wake
word, tells the Home Assistant integration, and serves the room's audio to it for one pipeline
run. The integration's entity, the ducking mixer, timers and the firmware's capture are other
tasks.

The hard part is P8's "one gap": K40 leaves the control plane unauthenticated, so a plain
streaming route on it could be read by any client on the network. The owner chose the
run-scoped route over a pinned secret, and named the secret as something that can be added
later.

## What was read

All on 2026-10-04 and 2026-10-05. No GPL source and no reciprocally licensed design was
opened.

- `docs/proposals/P8-voice-path.md` (Option A: "What", "Mute semantics", "Privacy (I4)", "Open
  inputs").
- `docs/decisions/0166-the-voice-role-on-the-wire.md`, `0167-the-wake-word-runtime.md`,
  `0169-the-microphone-intake.md`, `0151-controller-events-on-the-http-control-plane.md`,
  `0153-the-visualizer-stream-over-http.md`, `0136-a-server-identity-and-an-announce-command.md`.
- `docs/control-plane.md`, `docs/protocol.md` ("The voice role").
- `crates/server/src/{voice,events,control,conductor,session,config,main}.rs`,
  `crates/control/src/{catalog,zones}.rs`, `crates/wakeword/src/lib.rs`.
- The owner's answers of 2026-10-04 on the voice feature: the route is run-scoped; no voice
  intent targets a Soloist source (nothing here defines an intent); microphone hardware is
  deferred (everything here is tested against a scripted speaker).

## Decision

### The route's protection, as built

1. **Audio leaves the server only for an open run.** A run is opened by a `voice_start`
   command and by nothing else: a wake word opens none. With no run open the route answers
   `404` to everybody.
2. **A run has an identifier that only the peer that asked is told.** 16 bytes from
   `/dev/urandom`, written as 32 hexadecimal digits, new for every run, returned in the answer
   to `voice_start` (a `voice_run` message) and written nowhere else: no state, no event, no
   report, no metric, no log line, no `Debug` output. The route compares it in time that
   depends on its length alone, against every open run, and answers one `404` for "no run"
   and "not this one".
3. **The route answers one address.** The server is started with it
   (`--voice-integration <ip address>`); a caller from any other address is answered `403`
   before anything about a run is looked at, and a server started with none refuses every
   `voice_start` (`no-voice-integration`), because a run nobody may read should not light a
   speaker's status light. IPv4-mapped IPv6 addresses are compared as IPv4.
4. **A run has one reader, once.** The first request that offers the identifier from the
   registered address claims it; a second is answered `409`. A reader that closes, or takes
   nothing of what it is sent for 5 s, ends the run rather than leaving it to be claimed
   again.
5. **A run has a hard time limit**, 30 s unless `--voice-run-limit-ms` says otherwise (at most
   120 s), counted on the monotonic clock (`std::time::Instant`) from the moment it opens and
   never extended. It is checked on every pass of the event writer, which wakes for it, and
   again before every read and every write for the run's reader, so no sample is handed out
   or written at or after the limit; what the reader had been handed and not yet taken is
   overwritten.
6. **A run ends on mute and on voice disabled**: when its speaker reports its gate muted,
   when a frame of its speaker is dropped for any of 0169's reasons, when the conductor tells
   its speaker to stop the uplink, and when its speaker's session ends. Also on `voice_stop`
   and when another run is opened in its room. An ended run's queue is overwritten and
   emptied, and its identifier names nothing.
7. **While a run is open the room's speakers are told** (`voice_control`, `listening` on),
   which is what P8's "the room LED shows listening" needs from the server.

### What this is not, and the owner's open call

This is not authentication, and it is not as strong as a credential. Stated plainly:

- A source address on a LAN can be forged by a host on the same segment for a one-way
  packet, though holding a TCP connection from a forged address needs the forger to see the
  replies. The address rule narrows who can read; it does not prove who is reading.
- The identifier travels in clear text, in the answer to `voice_start` and in the request
  line of the route, as everything on this control plane does. A host that can read the
  traffic between the integration and the server can read the identifier, and, for that
  matter, the audio.
- Anybody on the network can send `voice_start` and `voice_stop`. They can light a status
  light for at most the limit and end a run; they are answered with an identifier they cannot
  use from their own address.
- The address is the server operator's configuration, not something the integration proves.

**The pinned-secret alternative is the owner's open call.** P8 names it: a pairing secret
created in the integration's config flow and pinned by the server on first use (the K92
pattern). It would close the first and third points above for the route (and could cover
`voice_start`), and it is a credential on the control API, which K40 says chorus does not
add; so it is the owner's to decide, and he has said it "can be added later without changing
the satellite". Nothing here forecloses it: it would be one more check in `serve_voice_audio`,
ahead of the run's identifier, and the speaker's side (0166) does not change.

### The cheap decisions

8. **The integration's address is registered with the server at start, by a flag, not by a
   control command.** P8's words are "the address the integration's config entry registered".
   A registration command on an unauthenticated control plane would let any client register
   its own address and then read a run, which would make the address rule decoration. A flag
   is how the server is already told the home automation's address for the other thing it
   trusts it with (`--announce-origin`, 0136). The cost is one more line of deployment
   configuration; where Home Assistant reaches the server through a bridge network, the
   address is the one the server sees. If the owner wants the integration to register itself,
   that needs the pairing secret above to mean anything.
9. **`voice_start` and `voice_stop` are catalog v2 commands naming a `zone`**, sent to
   `POST /api/command` like every other, so the integration's command path is unchanged.
   `voice_start` is the one command answered with something other than the state, because the
   identifier must not be in a message every subscriber is sent. Neither changes the room
   model, moves the serial or is persisted. `voice_stop` in a room with no run is accepted: a
   stop that races the limit is not an error.
10. **Refusals are named in the detail's first word**, as `no-players:` and
    `no-announce-origin:` are: `voice-disabled:` and `mic-muted:` on field `zone` (decided by
    the room model, so they are vectors), `no-voice-integration:` on field `t`. A room with no
    microphone is `mic-muted`: 0169 decided that such a room reads muted.
11. **The wake event is its own message on its own stream**, `voice_wake` on
    `GET /api/voice-events`, under the rules 0151 set for controller events: a bounded fanout,
    nothing replayed, nothing kept. It carries `zone` and `phrase`. It is not a state change
    (a wake word changes nothing the state describes) and it is not a controller event (the
    integration's event entities read that stream as button presses).
12. **The wake word is heard on the event writer's thread, not on a session's reader.** A
    reader appends to its buffer and wakes the writer with one non-blocking send; the writer
    copies out what no detector has heard and runs the models with no lock a session needs.
    So an inference delays no session and adds no thread (the population stays
    `6 + 2N + M`); what it can delay is an event stream, by the length of an inference. How
    long an inference takes on the server's hardware is not measured here, and no claim is
    made about it.
13. **One detector per model per session, and one event per utterance per room.** A second
    detection in a room within 2 s of the first is not reported (two speakers of a room both
    hear the phrase), and a room with a run open reports none. This is the rule 0169 left to
    this task for a room with two microphones: nothing is mixed, and the run reads the
    speaker that heard the phrase.
14. **A run reads one speaker**: the one that heard the room's wake word when the run opens
    within 5 s of it, else the first of the room with a live gate. A run opened after a wake
    word starts with what that speaker's buffer holds since the detection (at most the
    buffer's three seconds), because the command usually starts before the home automation
    has answered the event; a run opened without a wake word starts empty. Audio from before
    the wake word is never served.
15. **The route is raw PCM over a close-delimited HTTP response**:
    `application/octet-stream`, with `X-Chorus-Audio-Format: pcm_s16le; rate=16000;
    channels=1` saying what it is, which is the format Home Assistant's pipeline takes and
    the format on the wire from the speaker (0166), so nothing is converted. `audio/L16` was
    not used: RFC 3551 defines it as big-endian.
16. **The identifier is a query parameter of a `GET`.** The server logs no request line, the
    identifier is useless after the run and from any other address, and a header would need
    the control plane's request reader to keep headers it does not keep.
17. **The reader is one of the event writer's streams** (0153's pattern for a stream that is
    not fed by a fanout), so a run holds no control worker for its 30 s and counts against
    `--event-streams`. It gets no keepalive comment: the body is samples.
18. **The run's queue holds at most three seconds**; a reader slower than the microphone
    loses the oldest, counted in the run's last log line (`lost_bytes`).
19. **The state lists the models as `wake_words`** (`id`, `phrase`), written only when the
    build carries any, last in the message, never persisted. Every model runs in every voice
    room; a per-room choice is the integration task's to ask for.
20. **The log names rooms, speakers, reasons and counts**: `voice wake`, `voice run ...
    started`, `voice run ... ended reason=`, `voice route refused reason= peer=`. Never a
    sample, never an identifier.

## Compatibility

- A v2 state gains `wake_words` at its end on every server that carries a model, which is
  every server built from this tree. A reader takes the members it knows and ignores the rest;
  the committed state vectors other than the new `state-voice` are unchanged, because the
  room model has no models until the server says so. The v1 state shape is unchanged.
- Two new commands, two new messages the server sends, two new routes. Nothing that existed
  changes shape. The state file is unchanged (format 8).
- A server started without `--voice-integration` behaves as before this change, except that
  it hears wake words and publishes them.

## Considered and not chosen

- **A pairing secret pinned on first use.** Above: the owner's open call.
- **A registration command for the integration's address.** Decision 8.
- **Restricting `voice_start` to the registered address.** It would stop other hosts lighting
  the status light, and it is an authorisation rule on a command, which K40 and the owner's
  answer do not ask for. It belongs with the secret, if that is chosen.
- **Hearing the wake word on the session's reader thread.** No queueing between threads, and
  an inference between a speaker's messages.
- **A thread for the voice path.** A change to the declared thread population for work the
  event writer has time for.
- **The wake event on `/api/controller-events` or in the state.** Decision 11.
- **Serving the audio as server-sent events, base64 in `data:` lines.** A third more bytes
  and a decoder in the integration, for a stream that has one reader.
- **Holding a control worker for the run.** Simpler, and eight runs would take the whole
  default pool for 30 s.
- **Opening the run at the wake word, before the command asks.** Audio would be queued for a
  reader that may never come; the run is the integration's to ask for.

## ASSUMED values

None is a timing claim about audio, and none is measured: the run's default limit (30 s) and
its ceiling (120 s); the wake word's hold-off (2 s); how long after a wake word a run still
starts at it (5 s); the run queue's three seconds. The inference cost on the server's
hardware is not measured (decision 12).
