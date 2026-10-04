# 0124: a player thread runs one engine that fetches, decodes and writes into its port under actions; a next track is written straight after the current one through the same port; what is audible is reported from the port's own count; a wait on a stalled source is given up for a stop; and no command on the control API plays a URL

- Status: accepted (goal 16, 2026-10-03)
- Decided by: the goal (program section 20) inside the coordinator's goal-16 design envelope
  ("Renderer (track R)"), track `chorus-g16/renderer`, part R1; every number below not cited is
  ASSUMED
- Implemented in: `crates/server/src/mediaplayer.rs` (new: the engine, the handle, the pool),
  `crates/server/src/playersessions.rs` (new: a player tied to a group and to now-playing),
  `crates/server/src/main.rs` (the fetch policy and the drivers handed to `player::spawn`),
  `crates/fetch/src/http.rs`, `lib.rs`, `tls.rs`, `hls/mod.rs` (`open_cancellable`),
  `crates/server/tests/media_player.rs`, `audio-path.conf`, `.config/nextest.toml`,
  `docs/streams.md`, `docs/decoders.md`
- Builds on: ADR 0119 (player ports and threads), ADR 0120 (the fetcher), ADR 0121 (the UPnP
  core, whose effects and reports the engine is shaped to), ADR 0122 (the decoders)

## Context

After ADRs 0119 to 0122 the server had N player threads that did nothing (`IdleDriver`), a port
each, a fetcher, decoders and a pure AVTransport state machine. Nothing played a URL. This
record is the piece that does, and nothing more: no UPnP socket is opened here. The next part of
the track puts SSDP, SOAP and GENA on top.

Three constraints shaped it. The thread contract (ADR 0119): every thread exists before the
scheduling report, so the fetch and the decode run on the player's own thread and nothing is
created per stream. Line C of the goal: a gapless handover a listener cannot hear. Brief section
4.8: no arbitrary URL fetch but the input paths the decisions name.

## What was read

- `crates/server/src/player.rs`, `playerport.rs`, `tests/player_port.rs`, `control.rs`,
  `main.rs` (this repository, 2026-10-03).
- `crates/decode/src/{lib,decoder,media,resample,remix}.rs`, `crates/fetch/src/{lib,http,tls,
  policy,error}.rs`, `crates/fetch/src/hls/mod.rs`, `crates/upnp/src/avtransport.rs` (this
  repository, 2026-10-03); ADRs 0119, 0120, 0121, 0122; `docs/decoders.md`, `docs/streams.md`.
- The program brief [`.claude/goals/2026-09-chorus.md`](https://github.com/NSchatz/chorus/blob/535ed2816b5735153ac81c52a235024aa806f2b5/.claude/goals/2026-09-chorus.md) section 4.8 and section 20.
- `std::net::TcpStream::connect_timeout` and `set_read_timeout`
  (<https://doc.rust-lang.org/std/net/struct.TcpStream.html>, read 2026-10-03): a connect in
  progress cannot be left early; a read timeout surfaces as `WouldBlock` or `TimedOut`.
- No GPL or LGPL source was opened. Nothing of Symphonia beyond what `chorus-decode` exposes.

## Decision

### The engine is the driver of a thread that already exists

`Players::new(count, policy)` makes, before any thread starts, one `MediaPlayer` (a
`PlayerDriver`) and one cloneable `PlayerHandle` per player, and one channel all their reports
arrive on. `main.rs` hands the drivers to `player::spawn` in place of the idle ones. The
population is unchanged: `player-0` to `player-<N-1>`, and no thread is ever made for a stream.

A player's thread loops: take the actions waiting, do one piece of work (write what is pending
into the port, or decode one packet, or look at whether the last frame went out), publish the
position. Decoder output (interleaved f32 at the source's rate and channels) goes through
`chorus_decode::remix` to the port's channel count, then through a `Resampler` to the port's
rate, then into the port by `player::write_all`, which waits for room: decoded media is never
dropped. Before a start, a seek or a skip lets audio out, the port is held until 250 ms is
queued (`START_FILL_MS`) or the media ends.

### Actions and reports, and how they map to AVTransport

Every action is sent with the caller's epoch; every report carries the epoch of the last action
the player had taken. The renderer reads `AvTransport::epoch()` after an action and sends the
effects with it, and hands a report's epoch back to the matching method, which ignores a stale
one (ADR 0121).

| AVTransport effect | Action | | Report (`Event`) | AVTransport method |
|---|---|---|---|---|
| `Load { uri }` | `Load { uri, mime }` | | `Opened(info)` | `media_opened(epoch, info.duration_ms, info.seekable)` |
| `Start` | `Start` | | `Started` | `playing(epoch)` |
| `Pause` | `Pause` | | `Boundary(info)` | `track_boundary(epoch, info.duration_ms, info.seekable)` |
| `Resume` | `Resume` | | `NextFailed { reason }` | `next_failed(epoch, reason)` |
| `Stop` | `Stop` | | `Ended { played_ms }` | `ended(epoch)` |
| `SeekTo { ms }` | `Seek { ms }` | | `Failed { reason }` | `failed(epoch, reason)` |
| `QueueNext { uri }` | `QueueNext { uri, mime }` | | `PlayerHandle::position_ms()` | `position(ms)` |
| `ClearNext` | `ClearNext` | | `NextOpened(info)`, `SeekDone`, `SeekRefused`, `Title` | none (now-playing, logs) |
| `SkipToNext` | `SkipToNext` | | | |
| an empty URI | `Unload` | | | |

`mime` is what the control point's `protocolInfo` said; the response's own `Content-Type` is
preferred unless it is absent or `application/octet-stream`, and the bytes decide.

Where AVTransport's `ended()` returns `Load` + `Start` (a good next URI that was not joined),
the engine serves it without a second fetch: a `Load` whose URI is the queued next's takes over
the next's open source. Inside the engine that case does not arise by itself: on one thread a
next URI is either open when the current track's decoder ends (it joins), or failed
(`NextFailed`, then `Ended`), or arrives while the last frames are still going out (it is
opened then and joins them, late if the ring ran dry meanwhile, the dry ticks counted as
underruns). There is no "still opening" state on a single thread, so there is no timed wait.

`Stop` leaves the URI loaded and closes its source: a `Start` fetches it again (`Opened` is
reported again). A failure in mid-play lets what is queued play out and then reports `Failed`.

### Gapless: one port, written straight through

When the current decoder returns its end and the next track is open, the next track's frames
are produced into the same pending buffer and written after the last frame: no flush, no
padding. Remixing runs before the resampler, so the resampler depends on the source rate alone.

- The same rate: the same `Resampler` instance carries on. At the port's own rate it is a
  pass-through and the join is sample-exact; at another rate the join is continuous through the
  filter.
- Another rate: the resampler is flushed, which emits the frames it still owes as if silence
  followed (so the first track has exactly `ceil(n * to / from)` frames), and a new one is made.
  The join is still gap-free and frame-exact. It is not continuous through the filter: each
  side is band-limited against silence, not against its neighbour.

### What is audible is reported from the port's count

`PlayerPort::played_frames()` counts frames the audio thread took. The engine keeps, for each
join it wrote, the index of the next track's first frame (with a continuing resampler:
`origin + ceil(input_frames * to / from)`), and reports `Boundary` when the count passes it;
`Started` when it passes 0 after a start or a seek; `Ended` when it reaches everything written.
The count is looked at every `player::POLL` (5 ms) while the thread waits for room, and no read
of the source is started while the awaited frame is within 50 ms (`EVENT_HOLD_MS`) of going
out, so a source that stalls does not hold a report back. Measured on the host: `Boundary`
arrived with the count 73 to 466 frames past the join, inside the join's own chunk in every
pair (`docs/measurements/gapless-join-host.md`); the test asserts two chunks.

Until a join is audible, the caller's current track is still the one before it. A `Stop`, a
`Seek` or a `SkipToNext` in that window (at most what the ring holds, 1 s) puts the engine back
to the audible track, closed, with the joined one as the next again, so the engine and
AVTransport agree on what is loaded. A `QueueNext` in that window is taken as the track after
the joined one; `Boundary` carries the URI so the renderer can tell.

Position is `offset + (played - first frame of the audible track) * 1000 / rate`, clamped to a
known duration: frozen while paused (nothing is taken), 0 when stopped.

### A stop gets through a stalled source

A blocking read on the player's own thread cannot be interrupted from outside, and the fetch
policy's read timeout is 15 s. So `chorus-fetch` gains `open_cancellable(url, policy, cancel)`:
the connected socket waits in slices of `CANCEL_SLICE` (100 ms) under TLS and under the HTTP
framing, asking `cancel` between slices; rustls and the decoder see bytes or one final timeout,
never a partial state. `open` is unchanged for every other caller. The handle counts the
interrupting actions it sends (`Stop`, `Load`, `Unload`, `SkipToNext`), and `cancel` is true
while one is waiting.

The bounds:

- decoding, or waiting for room: an action is taken within `POLL` plus one decode step;
- waiting on a connected source that sends nothing: `Stop`, `Load`, `Unload` and `SkipToNext`
  within one slice (measured on the host: 48 ms for a stop, 55 ms for a load through a server
  that never answers; the test asserts 1 s); `Pause`, `Resume`, `Seek`, `QueueNext` and
  `ClearNext` wait for the read to end (data, or the read timeout and then `Failed`);
- inside a TCP connect or a name lookup: when it returns (the connect: at most the policy's
  `connect_timeout`, 10 s). `std::net` offers no way out of either.

What a listener hears after a stop is bounded separately by the ring: at most 1 s is queued,
and it is flushed when the stop is handled.

### The pool and the sessions

`Players` is the handles, the owners (`acquire(owner)`, `release(i)`, `owner_of(i)`,
`held_by(owner)`) and the report receiver, behind mutexes, with no thread. An idle player is
one with nothing loaded and no action on its way.

`PlayerSessions` is what a named input path calls to play a URL in a room: `play(request)`
acquires a player, issues `take` of the target with source `player:p<i>` (K78), sends `Load`
and `Start`, and writes the group's now-playing record (the caller's metadata, else the
stream's title, else the file's tags, else the station's name; buffering, then playing or
paused). `on_report` keeps the record in step and, on `Ended` or `Failed`, sets the group's
source to `none` (which clears the record), unloads the player and releases it; a failure's
reason is logged and kept (`last_failure`). `reconcile` releases a player whose group stopped
playing it, whoever changed the source. `pump` does both from a thread its caller already has.

### The fetch policy, and why no command plays a URL

`mediaplayer::fetch_policy(listener_ports, allow_loopback)` is built once in `main.rs`, before
the threads start, from the audio and control listeners' ports (the renderer's HTTP port is
one more entry), with loopback refused. Tests construct the engine in process with
`allow_loopback: true`. There is no flag or environment variable that relaxes it in the binary,
and `--media-allow-loopback` was not added: no test of the binary in this part needs it. The
next part decides whether its scripted control point does.

A `play_url` command on the control API would be an arbitrary-URL-fetch surface on an
unauthenticated port. Brief section 4.8 names the input paths that may fetch (UPnP renders, the
home automation's media and TTS URLs from its own address, stored alarm stream URLs) and that
is not one of them. So the engine is reachable in process only, and its tests are in process
(`crates/server/tests/media_player.rs`).

## The tests

`crates/server/tests/media_player.rs`, 27 tests, in the `wall-clock` nextest group (21.8 s
there, one at a time; 2.8 s under `cargo test`). A real v2 client session receives, sample for
sample: a WAV, a FLAC, an ALAC, an MP3, an Ogg Vorbis and an Ogg Opus file (against the
server's own decode put through the server's own rounding to 16 bits; the lossy two also within
ISO 11172-4 full accuracy of the fixtures' reference decodes); the gapless pairs (a generated
WAV pair cut at frame 70007, the FLAC pair, the MP3 and Vorbis pairs, and a 44.1 kHz then 48 kHz
pair); the next slot (replaced, cleared, not found, skipped to); pause and resume; stop and
start; seek (the first frame out is the frame asked for; a source without ranges is refused by
name); AAC and a loopback URL refused by name with nothing played and no connection made; a
stalled source; a live stream with ICY metadata; 44.1 kHz material on a 48 kHz server; and the
sessions.

## Not chosen

- **A thread per stream.** It is what a blocking fetch wants, and it is what the thread
  contract forbids: a thread made when a stream starts is one nobody checked.
- **Decoding on the audio thread.** A decoder allocates, a fetch blocks; the audio thread does
  neither.
- **A second thread per player for the fetch**, which would make every wait interruptible and
  keep the current track fed while a slow next URI opens. It doubles the fixed population for a
  case the ring covers for a second. Left for a measurement that shows the need.
- **Crossfade at a join.** It changes the audio of both tracks and hides exactly the defect the
  gapless tests look for.
- **Non-blocking sockets and a reactor** in the fetcher: a larger rewrite of ADR 0120 than a
  sliced wait, for the same bound on a connected socket.

## Deviations from the brief of this part

- `chorus-fetch` was changed (`open_cancellable`); the brief put the fetcher outside this part.
  Without it the stop bound would have been the read timeout.
- `Ended` carries `played_ms`, `Opened`/`Boundary`/`NextOpened` carry one `MediaInfo` (URI,
  duration, seekable, format, tags, station name).
- No "wait for a next that is still opening, then `Ended` and hand back" path (see above).

## ASSUMED

`START_FILL_MS` 250, `EVENT_HOLD_MS` 50, `IDLE_WAIT` 100 ms, `CANCEL_SLICE` 100 ms; the
`audio/L16` parameter rules (rate required, channels default 1) are from memory of RFC 3551.

## Follow-ups

- Opening the next URI holds the player's thread: an open slower than what the ring holds
  underruns the current track (counted). The same for a seek on a slow server.
- One open of a seekable file is two requests (the decoder sniffs, then seeks back to 0, which
  the fetcher serves with a range request).
- A port of 0 in `--listen` names nothing for the policy; the listeners are read from the
  configuration, not from the bound sockets.
- A stalled live stream resumes without refilling; no test holds a stream that stalls and
  resumes (the stall test stops it).
