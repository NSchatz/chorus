# 0041: protocol v2 keeps v1's frame, carries whole frames in records, and refuses v1 by name

- Status: accepted (goal 5, 2026-09-30)
- Decided by: the goal, on decisions K62 (encrypted sessions), K65 (the four roles), K92
  (adoption) and the brief's section 9 ("unknown types are skipped; v1 peers are refused by
  name")
- Implemented in: `crates/protocol/src/v2/` (catalog, codec, session), `crates/server`,
  `crates/client-linux`; specified in `docs/protocol.md` ("Message catalog", "The session",
  "Decoder behaviour", "Carrying this on a stream transport")

## Context

v1 is three messages in the clear on one TCP connection, framed as a type byte and a u16 length
(ADR 0003), with a decoder that skips unknown types by their length (ADR 0005). v2 must add a
session with encryption, hello and capabilities, a stream format, two codecs, telemetry and four
roles, and the C endpoint (goal 6) must implement it with a fixed frame budget. Three shaping
questions: does the frame change; how does encryption meet the frame; and what happens when a v1
peer meets a v2 one.

## What was read

- `docs/protocol.md` (v1), ADRs 0003, 0004, 0005 and 0016 in this repository.
- The Noise Protocol Framework, revision 34, section 3 (message format: at most 65535 bytes) and
  section 13 (application responsibilities: framing is the application's),
  <https://noiseprotocol.org/noise.html>, read 2026-09-30.
- `docs/research/2026-09-protocol-v2-sources.md` (the goal's research notes).

## Decision

1. **The frame does not change.** v2 is the same type byte and u16 length. v1's three messages
   keep their type bytes and their bytes (their v1 golden vectors are also their v2 vectors), so
   the sync and playout code that reads them runs unchanged inside a v2 session. New types are
   grouped: 0x10-0x15 session content, 0x20-0x24 the session's envelope, 0x30-0x37 the roles.
2. **Encryption is a frame, and a record carries whole frames.** After the handshake every frame
   on the connection is a `secure_record` (0x24) whose ciphertext is one or more whole v2 frames
   (at most 65519 bytes of plaintext: a Noise message is at most 65535 and the tag takes 16), with
   the record's own header as associated data. Options considered: (a) encrypt a byte stream in
   arbitrary records, frames spanning records; (b) whole frames per record. (b) is chosen: a
   receiver with a fixed frame budget (the endpoint) decrypts one record and decodes whole frames
   with no reassembly across records, frame boundaries inside a record are known, and so rule 3
   (skip an unknown type by its length) is safe inside a record even on TCP, where for v1 it was
   not (`docs/protocol.md`, "Where the skip rule stops applying"). The cost is that a frame larger
   than 65519 bytes cannot travel in a session; no v2 message needs one.
3. **Only the handshake, the refusal and the record travel in the clear.** Every other type found
   in the clear after the handshake, or any clear-only type found inside a record, ends the
   session: the session's envelope is not something to be forward-compatible about.
4. **A v1 peer is refused by name.** A v2 server whose first frame from a peer is a v1 message
   sends `session_refused` with reason `protocol_version` and a sentence naming v1, logs it, and
   closes; the v1 peer steps over the unknown refusal and sees a clean close. A v2 endpoint that
   gets no answer to `handshake_init` (a v1 server steps over it silently) refuses the server by
   name after its timeout. Keeping a v1 mode in the server was rejected: an unencrypted path
   beside the encrypted one would make "sessions are encrypted" (K62) a configuration rather than
   a property.
5. **v2 lives beside v1 in the library.** `chorus_protocol::v2` is a module next to the v1
   catalog, which stays as it is for the endpoint until it moves to v2 (goal 6).
   `fixtures/protocol/v2/` joins the shared-fixtures check with its C reader pending until then.
6. **The session layer hides the records.** `SecureWriter` seals whatever whole frames it is
   given; `SecureReader` hands v1's frames up as plain bytes and v2's other messages to a
   handler. That is what let the server's writer and reader threads and the Linux client's
   receive loop take v2 without being rewritten.

## Consequences

- An endpoint still on v1 (today's ESP32-S3 firmware) cannot stream from a v2 server; goal 6
  moves it. The refusal says why in both directions.
- The frame budget of a record (65519 bytes) is the largest v2 frame, 19 bytes less than v1's
  largest. The server refuses at startup, by name, a format and chunk duration whose
  `audio_chunk` would not fit (the default, 20 ms of stereo 16-bit at 48 kHz, is 3875 bytes).
- Adding a message type later is still a non-event for every decoder, now including on TCP.
