# 0000: a recording of the measurement sweep is uploaded as a 48 kHz mono 16-bit WAV to a route with a 2 MiB body bound of its own, fitted in memory and never kept; the route is told which sweep was played in its query; and a room's correction has one undo step, kept and persisted beside it

- Status: accepted, 2026-10-05
- Decided by: the task for the scope ("a route that takes one mono recording of the sweep for a
  room, runs the existing check and fit, and answers with the filters or a refusal by the
  fitter's own name; applying them through the existing room_eq path; and an undo that puts
  back the room's correction as it was before the last apply. The recording is never stored";
  "the upload format (WAV or raw PCM with a stated rate) and the size bound are the builder's
  choice, recorded"; "the route must know which sweep a recording is of"); K31 and K87 for
  room correction and the phone-microphone measurement; 0083 for the fitter; 0195 for the
  sweep's playback and the question it left open (a room measured with its correction on);
  0018 for the state file; audit findings B-4 and B-6 for the request bound and the POST
  rules. This record for the cheap decisions: the format, the bound, the query, the refusals,
  what counts as an apply and how deep the undo is.
- Recorded by: the owner's agent harness, in the pull request that adds this record.
- Implemented in: `crates/server/src/roomfit.rs` (`ROUTE`, `CONTENT_TYPE`,
  `MAX_RECORDING_BYTES`, `RATE_HZ`, `SweepSpec`, `parse_query`, `parse_wav`, `fit`, `Refused`),
  `crates/server/src/control.rs` (`is_upload`, `read_request`'s early return, `post_refusal`,
  `serve_room_fit`, `ControlState::room_fit_check`, `ControlState::room_fit`),
  `crates/control/src/catalog.rs` (`Command::RoomEqUndo`), `crates/control/src/zones.rs`
  (`Zone::room_eq_undo`, the `RoomEq` and `RoomEqUndo` arms of `apply`, the state's `undo`),
  `crates/control/src/persist.rs` (format 10, `load_room_eq_undo`); `docs/control-plane.md`
  ("Room correction: a recording, its fit and the undo"), `docs/room-correction.md` ("The
  recording: uploaded, fitted, applied, undone"), `audio-path.conf`, `fixtures/control/v2/`
  (`room_eq_undo`, `state-room-eq-undo`, `error-room-eq-undo-nothing`, `state-rich`); held by
  `crates/server/tests/room_fit.rs`, `crates/control/tests/room_eq_undo_v2.rs`,
  `crates/control/tests/catalog_v2.rs` and the unit tests of `crates/server/src/roomfit.rs`.

## Context

0083 gave chorus a fitter and 0195 a way to play its sweep in one room. Nothing took the
recording. The control plane's HTTP listener reads every request through one 16 KiB bound and
accepts two `POST` routes, both JSON; a recording of the 6.5 s `measure_sweep` plays is 624 KB
at 48 kHz, 16-bit, mono. And `room_eq` replaces a room's filters with no way back, so a fit
that sounds worse than what it replaced could only be undone by a person who had written the
old filters down.

## What was read

In this repository, on 2026-10-05. In full: `crates/server/tests/room_eq_bounds_agree.rs`,
`docs/room-correction.md`, `fixtures/roomfit/01-two-modes-one-null.params`,
`tools/conventions/check-adrs.sh` (the rule), `tools/build-dir.sh`. In part:
`crates/dsp/src/roomfit.rs` (`Sweep`, `RoomFitError`, `Fit`, `fit_recording`; the fitting
itself by name only), `crates/dsp/src/roomfit/synthetic.rs` (`wav_bytes`, `parse_wav`),
`crates/server/src/control.rs` (`ControlState`'s fields and constructor, `apply`, the
measurement hooks, `Request`, `Unreadable`, `DeadlineReader`, `read_request`, `post_refusal`,
`same_origin`, `refuse_unread`, `respond`, `serve_connection`, the unit tests of the bound),
`crates/server/tests/control_request_rules.rs` (its header and test names),
`crates/server/tests/common/mod.rs` (`RunningServer`, `http`),
`crates/server/tests/measure_sweep.rs` (its header and helpers),
`crates/control/src/catalog.rs` (`Command`, its name, zone and encoder arms, the `room_eq`
decoder, `Refusal`), `crates/control/src/zones.rs` (`Zone`, the `apply` arms for sound,
`measure_check`, the state's encoder for a room, `zone_list`), `crates/control/src/sound.rs`
(`RoomEq`), `crates/control/src/persist.rs` (the format history, `render`, `load_zone`,
`load_sound`, `Section`), `crates/control/tests/catalog_v2.rs` (the vector harness),
`docs/control-plane.md` (the v2 command table, "Per-room sound", "Room correction: the
`measure_sweep` command", "How the messages travel", "What a request may be", "What a command
must carry", "What survives a restart"),
`docs/decisions/0195-the-measurement-sweep-plays-on-a-stream-of-its-own.md` (its header,
"What was read", "Considered and not chosen", "ASSUMED values", "Follow-ups"),
`audio-path.conf` (the server's exclusions and the room-correction entry), `fixtures/README.md`
(the control vectors).

Outside this repository, on 2026-10-05:

- The Fetch standard, https://fetch.spec.whatwg.org/, "CORS-safelisted request-header": a
  `Content-Type` is safelisted only when its MIME type's essence is
  `application/x-www-form-urlencoded`, `multipart/form-data` or `text/plain`. So `audio/wav`
  from a page on another origin needs a preflight, which this server never approves.
- RFC 9110, HTTP Semantics, https://www.rfc-editor.org/rfc/rfc9110.html, section 15.5: the
  names and meanings of `411 Length Required`, `413 Content Too Large`, `415 Unsupported Media
  Type` and `422 Unprocessable Content` ("understands the content type ... but was unable to
  process the contained instructions").
- P. Kabal, "Audio File Format Specifications: WAVE", McGill University,
  https://www.mmsp.ece.mcgill.ca/Documents/AudioFormats/WAVE/WAVE.html: the RIFF header, the
  `fmt ` chunk's fields and offsets, `WAVE_FORMAT_PCM` = 1, the pad byte after a chunk of odd
  length, that `LIST` and `fact` chunks may appear, and that a reader must not assume a
  44-byte preamble.

No GPL source and no reciprocally licensed design was opened. Nothing was measured in a room:
the only recordings the route has been given are the synthetic fixtures.

## Decision

1. **The upload format is a WAV file**: RIFF WAVE, integer PCM (format 1), one channel, 16
   bits, 48 000 Hz, sent as the body of `POST /api/room-fit?zone=<room>` with
   `Content-Type: audio/wav`. The reader walks the chunks, needs `fmt ` before `data`, steps
   over any other chunk, and checks every length against the bytes there are. The rate is in
   the file, where it cannot disagree with a header beside it; the fixtures are already this
   format, so the route is tested with the files the fitter is tested with, byte for byte.
2. **One rate**: 48 000 Hz, the rate every fixture was made at. Another rate is refused
   `unsupported_rate`; the recorder resamples. A second rate is added with a fixture at it.
3. **The body's bound is 2 MiB** (`MAX_RECORDING_BYTES`), checked against the declared
   `Content-Length` before a byte of the body is read: `413` over it, `411` with no length. It
   is 21.8 s of audio in this format, three times the 6.5 s `measure_sweep` plays. The head of
   the request is read through the 16 KiB bound every request has, and `read_request` leaves
   the body unread for this one method and path only, so every other route, and this path
   under another method, keeps the one bound. The request's 5 s deadline covers the body.
4. **The POST rules are the command route's**, with `audio/wav` for `application/json`: `415`
   for another type, `403` for an `Origin` that is not this server's own, both from the head.
   `audio/wav` is not CORS-safelisted (the Fetch standard, above), so the reasoning of audit
   finding B-6 holds unchanged.
5. **The route is told which sweep was played in its query**: `sweep_ms` (1000 to 10000) and
   `fade_in_ms` (0 to 1000, shorter than the sweep). With neither it is the sweep
   `measure_sweep` plays, `Sweep::recommended`: 5000 and 100. With `sweep_ms` alone the
   fade-in is 0, which is the fixtures' 1 s sweep. The frequencies and the level are always
   `Sweep::recommended`'s. An unknown member, a member given twice or a number out of bounds
   is refused `bad_query`: fitting against the wrong sweep yields filters for no room.
6. **The fit is the fitter's**: `fit_recording` with `Target::flat()` and
   `FitConfig::default()`. The answer is `{"v":2,"t":"room_fit","zone","sweep_ms","filters",
   "rms_before_db","rms_after_db"}` with the filters written by the fitter's own
   `filter_json`, so they are a `room_eq`'s `filters` as they are. The fitter's refusals are
   answered `422` under its own names and in its own words (`too_short`, `clipped`,
   `too_quiet`, `too_noisy`); every refusal is the catalog's `error` message.
7. **The upload changes nothing.** Applying is the existing `room_eq` command, sent by the
   controller once a person has seen the filters.
8. **A room whose correction is switched on is refused** (`409`, `correction_on`): 0195's open
   question. The sweep such a room played was the corrected room, and its fit would replace
   the correction with one for the residue. The order is: switch off, measure, upload, apply.
   The check reads the room as it is when the upload arrives, which is the only moment the
   route knows.
9. **One recording is fitted at a time** (`503`, `busy`): the fit is the one piece of work on
   this listener that takes a core for a while, and it runs on the control worker that read
   the request. No thread is added.
10. **The recording is never stored.** It is a `Vec<u8>` on the worker's stack frame, borrowed
    by the fit and dropped. `crates/server/src/roomfit.rs` opens no file and prints nothing;
    the route prints one line, the outcome's, which carries the room, the sweep's length, the
    sample count, the rate and how it ended. The test on the real binary shows the state
    directory's files unchanged byte for byte and the output short.
11. **The undo is one step deep.** A `room_eq` that carries `filters` is an apply: the room's
    `RoomEq` as it stood (filters and `enabled`) is kept in `Zone::room_eq_undo`, replacing
    any step kept before. `room_eq_undo` puts it back and spends it; with none kept it is
    refused `nothing-to-undo`. A `room_eq` that carries only `enabled` is not an apply. A
    never-corrected room's "before" is the default (no filters, enabled), so an undo after a
    first apply leaves no correction.
12. **The state says whether there is one**: `"undo":true` in the room's `room_eq`, written
    only while a step is kept, so the state of a room nobody corrected is byte for byte what
    it was.
13. **The step is persisted as `room_eq` is**: state-file format 10 adds `room_eq_undo`
    (`none`, 0 or 1) and `room_eq_undo_filters` to `[zone]`, both required; formats 1 to 9
    load with nothing to undo.

## Considered and not chosen

- **Raw PCM with the rate in the query or a header.** Smaller by 44 bytes and no parser, but
  the rate, the width and the channel count would each be a claim beside the data that
  nothing checks, and the fixtures would need a second encoding.
- **Compressed uploads (Opus or AAC from `MediaRecorder`).** A lossy codec changes the
  response being measured, and it would put a decoder in the path of an upload.
- **Raising the one request bound to 2 MiB.** Every route would then read 2 MiB from any
  peer; audit finding B-4 is why the bound is small.
- **`multipart/form-data`.** It is one of the three types a page elsewhere can send without a
  preflight, which is exactly what the content-type rule exists to refuse.
- **Fitting on top of an enabled correction**, by composing the old filters with the new
  inside 8 filters. It needs a merge rule the fitter does not have. Refusing is the rule that
  cannot give a wrong correction.
- **Switching the correction off inside `measure_sweep`.** That is playback's command, out of
  this task's scope, and it would change a room's sound as a side effect of a measurement.
- **Applying on upload.** A person sees the filters first; and one command path for changing
  `room_eq` means one place where the undo is kept.
- **An undo stack, or a redo.** The task asks for the correction "as it was before the last
  apply". A stack needs a bound, a persisted list and a way to show it; a redo doubles the
  state for a step the person can retake by applying the filters they were just shown.
- **Counting `enabled`-only commands as applies.** Switching a new fit off and on to compare
  it would then spend the undo on the comparison.
- **An `undo` member in every room's `room_eq`, true or false.** Clearer to read, and it
  changes every committed state vector and every consumer's fixtures for a member most rooms
  never use. The state already writes `wake_words` and `measurement` only when there is one.
- **A longer deadline for the upload.** 2 MiB in 5 s is 3.4 Mbit/s, and the route is for a
  phone on the house's own network. Not measured; a follow-up if a real phone misses it.
- **Keeping the last recording for diagnosis.** It is a microphone recording of a room in a
  home. The task says never.

## ASSUMED values

- `MAX_RECORDING_BYTES` = 2 MiB: three times the program `measure_sweep` plays. No phone's
  recording has been sized against it.
- `SWEEP_MS` = 1000 to 10000 and `FADE_IN_MS_MAX` = 1000: the fixtures' sweep at the low end,
  twice the recommended sweep at the high end, which with a second of response still fits the
  body bound.
- `fade_in_ms` defaulting to 0 when `sweep_ms` is given: the only sweep whose fade-in is
  known without being said is the recommended one.
- One fit at a time: on this development host a fixture's fit took well under a second in an
  unoptimised build (nine real-binary tests with twenty-odd fits finished in 1.4 s); not
  measured on the target, and not a timing claim about anything.

## Follow-ups

- The app's measurement screen (out of scope here: no `web/` change).
- A real phone's recording through the route, and its report in `docs/measurements/`.
- A second sample rate (44 100 Hz), with a fixture at it.
- Averaging several sweeps or seats, detrending and per-model calibration
  (`docs/room-correction.md`, "For goal 22").
- Letting a person verify a correction by measuring with it switched on, which the
  `correction_on` refusal does not allow today.
