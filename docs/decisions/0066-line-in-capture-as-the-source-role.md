# 0066: a Linux endpoint's line-in is the source role: captured through ALSA, offered by signal presence, sent upstream as PCM stamped at its capture instant through the sync offset

- Status: accepted (goal 10, 2026-09-30)
- Decided by: the goal (brief section 14 item 3; K65, K70, K94, I4, brief section 4.8)
- Implemented in: `crates/alsa/src/lib.rs` (capture: `Pcm::read`'s overrun report,
  `capture_delay_frames`, `avail_frames`), `crates/client-linux/src/source.rs` (the role),
  `crates/client-linux/src/session.rs` (`hello` roles, the first offer, `source_control`
  delivery), `crates/client-linux/src/config.rs` (the `--line-in` flags), `crates/client-linux/src/sync.rs`
  and `run.rs` (the published offset), `crates/client-linux/src/main.rs` (wiring,
  `--probe-line-in`); held by `crates/client-linux/tests/line_in_source.rs`, the unit tests in
  `source.rs` and `config.rs`, and the line-in probe on ALSA `null` in `tools/alsa-null-run.sh`
  (`make verify-alsa-null`, a gate step)

## Context

K65 gives protocol v2 a source role: any endpoint can offer an input (line-in, optical, HDMI ARC)
as a source any room or group can play; K70 gives the streaming amp a line-in and an optical in.
Goal 5 built the messages (`source_offer` 0x36, `source_control` 0x37) and `docs/protocol.md`
says what an endpoint does with them, but no endpoint captured anything: `crates/alsa` could
open a capture PCM for the measurement harness and nothing else used it, and `chorus-client`
declared the player role only. Goal 11 (a line-in's buffer grows without a glitch as rooms join,
K94) and goal 17 (line-in sharing to any group) build the server's routing on top of this.

## What was read

All read 2026-09-30:

- alsa-lib's PCM interface documentation (its doxygen pages only; alsa-lib is LGPL and its source
  was not opened): <https://www.alsa-project.org/alsa-doc/alsa-lib/group___p_c_m.html> for
  `snd_pcm_delay` on capture ("the time that a frame that was digitized by the audio device takes
  until it can be read from the PCM stream shortly after this call returns. It is as such the
  overall latency from the initial ADC to the read call"), `snd_pcm_avail` ("a positive number of
  frames ready to be read (capture)"), `snd_pcm_readi` ("-EPIPE: an overrun occurred") and
  `snd_pcm_recover`; <https://www.alsa-project.org/alsa-doc/alsa-lib/pcm_plugins.html> for the
  Null plugin (capture "creates a stream with zero samples").
- `docs/protocol.md` ("Source", "The four roles", "The session, in order", 0x02, 0x03, 0x12),
  `crates/protocol/src/v2/{catalog,messages,session}.rs`, ADR 0008 (the dlopen binding) and ADR
  0050 (the ALSA `null` runs), `crates/client-linux/src/{session,run,sync,sink,config,main}.rs`,
  `crates/server/src/session.rs` (it accepts and ignores source messages today), the brief's K65,
  K70, K94, I4 and section 4.8.
- RFC 5905 section 4 (the offset and its bound of half the round trip), as already cited in
  `crates/client-linux/src/sync.rs`.

No GPL or LGPL source was opened.

## Decision

1. **Capture stays in `crates/alsa`**, the one crate allowed `unsafe` for the sound stack
   (conventions rule 2's list is unchanged: no new file needs FFI). `Pcm::read` now returns a
   `ReadReport` whose `overran` is the device's own signal (`-EPIPE` from `snd_pcm_readi`), and
   capture gains `capture_delay_frames` (`snd_pcm_delay` on a capture stream, `None` in the
   overrun state) and `avail_frames` (`snd_pcm_avail`, one more symbol looked up at load).
   `crates/measure`'s capture reads the new field names.
2. **The role is one module, `crates/client-linux/src/source.rs`,** behind a `CaptureSource`
   trait with one shipped implementation, `AlsaCapture`. The modelled capture device lives under
   `tests/` only, for the reason `sink.rs` gives: no flag selects a pretend input.
3. **Configuration is additive:** `--line-in <alsa capture device>`, with `--line-in-name`,
   `--line-in-kind` (`line_in`, `optical`, `hdmi_arc`), `--line-in-rate-hz` (default 48000),
   `--line-in-channels` (1 or 2, default 2) and `--line-in-format` (default `pcm_s16le`). A
   describing flag without `--line-in` is refused by name, and so is `mic` or any other kind: a
   speaker microphone is never a source (I4, brief section 4.8). One input per endpoint, source
   id 1, for now.
4. **The session:** with a line-in, `hello` declares `player | source` (a source endpoint is still
   a player), and a `source_offer` with `signal` false follows `capabilities` at once: nothing
   has been measured yet. The device is then read continuously, one 20 ms chunk at a time
   (`LINE_IN_CHUNK_MS`, ASSUMED), started or not, because presence is measured on it.
5. **Signal presence is level over a window with hysteresis** (every value ASSUMED, named in
   `source.rs`, not measured on any input): the RMS of a 100 ms window at or above -50 dBFS makes
   the signal present at once; it is withdrawn only after the level has stayed below -60 dBFS for
   2 s, so a pause between tracks does not withdraw a source; a window between the two changes
   nothing. Every change is a new `source_offer`.
6. **A `start` is checked by name before anything is sent.** The input must be the one offered
   (`unknown-source`), the codec must be one the endpoint listed in `capabilities`
   (`codec-not-listed`), and it must be one the endpoint can send an input in
   (`codec-not-sent`). This client encodes nothing, so it sends PCM only: it lists FLAC and Opus
   because it plays them, and a start in either is refused with `codec-not-sent`. A refusal is a
   `source-start-refused` log line with the reason and a counter; no `stream_format` follows and
   the input stays offered. `docs/protocol.md` is clarified to say so. There is no wire message
   for a refusal; the server sees no `stream_format` (goal 17 may want one, below).
7. **An accepted start sends `stream_format`** (PCM, the capture's rate, format and channels, map
   `FL FR` or `MONO`, frames per chunk), then one `audio_chunk` per chunk captured after the
   start, sequences from 0; `stop` sends `stream_end` with the final chunk's sequence and one
   configured chunk duration past its timestamp, as 0x03 defines it.
8. **A chunk's timestamp is its capture instant on the server timeline.** Right after a read of
   `n` frames returns at `now` on the endpoint's `MonotonicTimeline` (the clock the sync exchange
   stamps on), the capture delay `d` says the next frame was digitized `d` frames ago, so the
   first of the `n` was digitized at `now - (d + n) / rate`; the stamp is that plus the offset
   the playout loop publishes (`server = client + offset`, `sync::PublishedOffset`, carried on the
   session's `Counters`). No wall clock is read; `source.rs` is on the audio path in
   `audio-path.conf`. Chunks captured before any offset is known are held (at most 1 s, ASSUMED;
   the oldest dropped and counted as `dropped_no_offset`) and stamped when one is: a timestamp is
   never invented.
9. **Overruns are counted and logged, never hidden:** each one is a `source-overrun` line with the
   running count, the count is in the `source ...` status line at the end of a session, and the
   next chunk's timestamp, taken from its own capture instant, shows the lost frames as a gap.
   The sequence carries on (a sequence gap would read as lost network frames).
10. **One writer per session.** The time-sync exchange and the source role share the session's
    `SecureWriter` behind a mutex (`SharedWriter`), a whole frame per lock, so records never
    interleave. The playout loop never waits behind the source: its time-sync request (one whole
    frame per write) is dropped and counted when the writer is busy, which costs one exchange
    rather than a stalled playout. The server's `source_control` messages reach the role on a channel from the
    session's message handler.
11. **The server is unchanged.** It already accepts a peer declaring the source role and ignores
    its source messages; routing a shared input to rooms is goals 11 and 17. The end-to-end test
    therefore uses a scripted server peer on the real server-side handshake and records.

## Tests (fakes only, conventions rule 10)

`crates/client-linux/tests/line_in_source.rs`: the real `session::open`, the real role on a
modelled capture device (a script of 1 s silence, 3 s of a 441 Hz tone, 3 s silence, released by
the test a chunk at a time, stamped on a clock the test controls, a fixed modelled capture delay,
one scripted overrun of 480 frames), and the real playout (`receive::handshake`, `run_session`)
on the same encrypted loopback session. It checks: `hello` has `player | source`; the first offer
has no signal; a second of silence sends nothing; the tone's first window offers the signal; a
FLAC start is refused by name (`codec-not-sent`) and a PCM start sends `stream_format` first;
chunks captured before an offset is known are held, not sent; every chunk upstream is the
captured frames bit for bit, in sequence, stamped at exactly its capture instant plus the
published offset; the overrun is counted, logged and visible as a 30 ms step between two
timestamps; the offer is withdrawn 2 s into the silence; `stop` yields `stream_end` naming the
last chunk; and the playback beside it plays to its own `stream_end` with its sync exchange
answered over the shared writer, its estimated offset within 50 ms of the true one. The stamps
are held to an offset the test publishes, not to the playout loop's live estimate, so they can be
checked exactly; in the binary both roles share one `Counters`. Every wait in the test has a
deadline, and the whole test exits failed after 180 s whatever it waits on (a first draft
deadlocked on its own mutex and held a gate run for an hour and a half). None of this is timing evidence
(BRIEF §3.1 rule 3): it shows the stamps are computed as specified, not how well a real device
keeps time.

`make verify-alsa-null` (a gate step) adds `chorus-client --probe-line-in --line-in null`: the
shipped binary opens ALSA's `null` capture device through the dlopen binding, reads 200 ms (9600
frames), reads the capture delay, and must report no overrun and no signal. Where there is no
libasound the step refuses by name (and prints SKIPPED under CI), as it already did.

## Not chosen

- **An encoder on the endpoint** (FLAC or Opus upstream): a line-in on a wired Linux endpoint is
  cheap as PCM, and the source role can add an encoder later without changing the wire; refusing
  by name keeps "one the endpoint listed" honest in the meantime.
- **Stamping at send time or with a wall clock:** the stamp would carry the network and scheduling
  jitter, and a wall clock breaks guardrail 4.
- **A sync exchange of the source role's own:** two offset estimates on one endpoint could
  disagree; the source reuses the playout loop's.
- **A new wire message for a refused start:** the catalog is goal 5's and the server does not
  route sources yet; a follow-up for goal 17.
- **Peak-level detection, or a single threshold:** peak trips on clicks; one threshold chatters on
  a quiet passage. RMS over a window with two thresholds and a hold does neither.

## Follow-ups

- Goal 11: the server accepts a source (starts it, buffers it, grows the buffer when rooms join
  without a glitch, K94) and the source-only endpoint (no playback stream, hence no offset until
  the server sends one) gets a time-sync exchange of its own before its first start.
- Goal 17: line-in sharing to any group; whether a refused start needs a wire message.
  (Done in ADR 0129, 2026-10-03: a line-in plays in any number of groups, and a refused start
  gets no wire message, with the reasons.)
- Measurement on hardware: the signal thresholds, the chunk and the capture ring on a real
  line-in, and the capture delay's accuracy on the rack amp's codec, when the board exists.
