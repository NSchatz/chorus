# 0000: protocol v2's voice role is bit 5 of hello with three messages of its own; microphone audio is 16 kHz mono 16-bit little-endian PCM in `mic_audio`, never a source stream, and a role message from a session that did not declare its role is refused

- Status: accepted, 2026-10-04
- Decided by: the owner for the path (proposal P8, Option A, approved: the speaker sends
  microphone audio to chorus-server over the encrypted speaker protocol, the server runs the
  wake word and Home Assistant's pipeline takes 16 kHz mono 16-bit PCM; K71, K73, K67, I4); the
  task for the messages ("the wire format of the mic audio is the builder's choice, recorded in
  an ADR"); this record for the format, the message shapes, the bounds and the role rule's
  place in the library.
- Recorded by: the owner's agent harness, in the pull request that adds this record.
- Implemented in: `crates/protocol/src/v2/catalog.rs` (`roles::VOICE`, `Type::MicAudio`,
  `Type::MicState`, `Type::VoiceControl`, `Type::role`, `MicGate`, `mic_format`, `MIC_*`),
  `messages.rs`, `codec.rs` (`decode_frame_for_roles`, `DecodeError::RoleNotDeclared`);
  `firmware/include/chorus/protocol_v2.h`, `firmware/src/protocol_v2.c`
  (`chorus_v2_type_role`); `docs/protocol.md` ("The voice role"); held by
  `crates/protocol/tests/v2_voice.rs`, `v2_vectors.rs`, `v2_rules.rs` and
  `firmware/tests/test_protocol_v2.c` (`make -C firmware golden-vectors`) on the shared vectors
  `fixtures/protocol/v2/{mic_audio,mic_state_*,voice_control_*,hello_voice}` and
  `fixtures/protocol/v2/rejected/mic_audio_*`.

## Context

P8 makes a chorus room with a microphone a Home Assistant voice satellite through chorus's own
integration. The first leg is the speaker to chorus-server, inside the session every speaker
already has. Protocol v2 had nothing for it: its catalog carried audio upstream only as a
source (`source_offer`, `source_control`, then `stream_format` and `audio_chunk`), and I4 says
a speaker microphone is never a source. The speaker's side already has a gate
(0063: `chorus_controls_mic_pass`, closed until the hardware switch reads live), with no way
to tell the server its state, and the server had no way to say "voice is enabled for this
room" or "the room is listening" (the status light).

This record is the wire only. Nothing sends or acts on the messages yet: the wake word, the
run, the server's route to Home Assistant, the firmware's capture and its light are later
tasks.

## What was read

All on 2026-10-04. No GPL source and no reciprocally licensed design was opened.

- `docs/proposals/P8-voice-path.md` (Option A; its table row on the pipeline's input:
  "STT fixed at 16 kHz 16-bit mono PCM", Home Assistant core 2026.9.3,
  `assist_satellite/entity.py:512-516`, Apache-2.0, as P8 cites it; K73 quoted there:
  "speakers stream mic audio to chorus-server only while unmuted and voice is enabled").
- `docs/protocol.md` (the frame, hello and roles, "The four roles", "Source" with the goal-10
  and goal-11 clarifications, "Decoder behaviour", "Golden vectors", "Carrying this on a
  stream transport").
- `docs/decisions/0063-controls-led-and-mic-gate.md` (decision 6: the gate starts closed and
  the switch also breaks the microphone's supply), `0041-protocol-v2-framing.md`,
  `0066-line-in-capture-as-the-source-role.md`.
- `crates/protocol/src/v2/` and `crates/protocol/tests/`, `firmware/src/protocol_v2.c`,
  `firmware/include/chorus/protocol_v2.h`, `firmware/tests/test_protocol_v2.c`,
  `fixtures/README.md`, `tools/conventions/check-shared-fixtures.sh`.
- The owner's answers of 2026-10-04 on the voice feature: the server's streaming route to the
  integration is run-scoped (not part of this wire); no voice intent targets a Soloist source;
  microphone hardware is deferred. None changes a byte here.

## Decision

1. **A sixth role bit, `voice`, bit 5 of `hello.roles`.** An endpoint with a microphone
   declares it. It is separate from `source` on purpose: a server that listens to sources
   listens to nothing of a voice endpoint, and the reverse.
2. **Three new types after `sound`, in the role block: 0x3A `mic_audio`, 0x3B `mic_state`,
   0x3C `voice_control`.** All travel in records, as every role message does, so microphone
   audio is encrypted and authenticated by the session (K62) with nothing added.
3. **`mic_audio` is its own message, not an upstream `audio_chunk`.** An upstream
   `audio_chunk` is a source's audio and reaches whatever plays that source. Microphone audio
   under the same type would be told apart only by session state (which `stream_format` came
   last), so one bookkeeping mistake would play a room's microphone into another room. A type
   of its own makes that mistake unrepresentable: no code path that takes `audio_chunk` can
   be handed a microphone's samples. It also needs no `stream_format` exchange.
4. **The audio is 16 kHz, mono, 16-bit signed little-endian PCM (`format` 1,
   `pcm_s16le_16k_mono`), uncompressed.** Reasons:
   - It is exactly what the pipeline takes (P8's table), so the server passes bytes through:
     no resampler and no decoder between the speaker and the pipeline, and none to drift.
   - What a wake-word model hears is what the microphone captured. A lossy codec in front of
     it changes its input in a way nobody has measured. ASSUMED: the permissively licensed
     wake-word models K73 names take 16 kHz mono; the wake-word task confirms it or converts
     on the server, and the wire does not change either way.
   - It costs the endpoint no encoder. An ESP32-S3 that decodes Opus for playback would also
     have to encode it (ASSUMED costly; no chorus measurement), and FLAC saves little on
     speech at this rate for the same reason it needs a codec at all.
   - The bytes are few. 16000 samples a second of 2 bytes is 32000 bytes a second (256 kbit/s)
     of data; at the nominal 20 ms chunk that is 50 frames a second of 656 bytes. Arithmetic,
     not a measurement; a wired or Wi-Fi speaker's link is not the constraint.
   - One rate conversion at most, in one place: an endpoint whose ADC runs at another rate
     decimates before it sends, beside the capture.
   - Samples are little-endian because `audio_chunk`'s PCM is, and the pipeline's is.
5. **`format` is a byte, with one value defined.** A later layout (a second channel for an
   echo reference, a codec) takes the next value without a new type; a decoder that does not
   know it rejects that frame by name (`format`, undefined) and carries on. 0 is not a format.
6. **`mic_audio` carries `sequence` (u32, 0 at each start of the uplink, wrapping) and
   `timestamp_ns` (u64, the capture instant of the first sample on the server timeline).** The
   sequence shows loss, which a wake-word buffer must know about. The timestamp is stamped as
   a source chunk's is (0066): the endpoint's monotonic capture time through its sync offset,
   never a guess. It is on the server timeline so that a later echo canceller can line the
   microphone up against what the same room was playing, whose chunks carry that timeline
   too. Nothing reads it yet.
7. **A chunk is 1 to 1600 samples, whole.** 320 samples (20 ms) is nominal; 1600 (100 ms) is
   the bound, so a receiver's buffer for one message is 3200 bytes. Both ASSUMED: chosen to
   keep added latency small beside a network round trip, not measured. Half a sample and an
   over-long chunk are rejected, never trimmed: dropping a byte would shift every sample
   after it into noise.
8. **`mic_state` is one byte, `gate`: 0 `muted`, 1 `live`.** It reports 0063's gate, which
   stays the endpoint's alone: the protocol has no message that opens it. Sent once after
   `capabilities` and at every change; a server that has none treats the endpoint as muted.
   When the gate closes, the `mic_state` follows the last `mic_audio`.
9. **`voice_control` is two bools, `uplink` and `listening`, whole state.** `uplink` is K73's
   "voice is enabled": the server's half of the condition, the gate being the other. A
   session starts with it off, so a speaker sends no microphone audio to a server that never
   asked. `listening` drives the status light (a run is open in the room) and is separate
   because a room can be listening through another of its speakers. One message for both
   because both are the server's word to one voice endpoint and change together at a run's
   start and end.
10. **The role rule is in the library.** `Type::role` (C: `chorus_v2_type_role`) names the
    role of each role message, and `decode_frame_for_roles(buf, session_roles)` rejects a
    well-formed role message on a session whose endpoint did not declare the role
    (`DecodeError::RoleNotDeclared`), costing that frame only. `docs/protocol.md` already said
    a role's messages go only to peers that declared it and that a peer skips the others;
    this makes the rule one call, and it is what refuses `mic_audio` from a peer without the
    voice bit. `decode_frame` itself is unchanged, so no existing caller's behaviour moves.
    The C library has the table and not the wrapper: the endpoint is never sent `mic_audio`,
    and its session already steps over role messages it did not declare.

## Compatibility

- The three types are unassigned to every earlier decoder, which skips them by their length
  prefix (decoder rule 3). Both test files show it with the v1 decoder.
- Role bit 5 is not skippable: `hello`'s undefined bits are rejected, so a server built
  before this record rejects a voice endpoint's `hello`. Chorus's server and endpoints are
  built from one tree, and an endpoint declares `voice` only when it has a microphone and
  firmware that sends these messages, which no released firmware does. No existing vector's
  bytes change.

## Considered and not chosen

- **`source_offer` with a `microphone` kind.** Forbidden by I4 and by the source section;
  it would make a microphone an input any room can play.
- **`stream_format` plus `audio_chunk` at 48 kHz, as a source.** Reuses code, and conflates
  the two streams (decision 3); also three times the bytes for audio the pipeline resamples
  down.
- **Opus in `coded_chunk`.** Smaller, at the cost of an encoder on the endpoint, a decoder on
  the server and a lossy stage in front of the wake word. `format` leaves the door open.
- **`timestamp_ns` on the endpoint's own clock.** Works without a sync offset, but puts the
  mapping on the server for every chunk, where the source path already does it on the
  endpoint.
- **The mic state in `telemetry` or `controller_state`.** `telemetry` is periodic, and a mute
  must be known at once; `controller_state` is server to endpoint.
- **Separate uplink and listening messages.** Two types for two bytes that change together.

## ASSUMED values

The 20 ms nominal chunk and the 1600-sample bound; that the wake-word models take 16 kHz
mono; that encoding on an ESP32-S3 is costly enough to matter. None is a timing claim made
by this record, and each is settled by the task that first sends or reads the audio.
