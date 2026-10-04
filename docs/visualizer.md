# The visualizer stream

A light stream for speaker LEDs and, through Home Assistant, lights (K65): levels, bands, beat
and colour, computed on the server from the audio each stream slot plays, and sent to every
endpoint of the playing room or group that declared the `visualizer` role, stamped on the server
timeline at the moment that endpoint's room HEARS the audio they describe. The wire messages are
`docs/protocol.md` "Visualizer and colour" (0x34 `visualizer_frame`, 0x35 `color`); the decision
record is `docs/decisions/0084-the-visualizer-stream.md`; the research is
`docs/research/research-dsp-phase-b.md` section 2 (read 2026-10-01). A subscriber of the HTTP
control plane that holds no audio-wire session (Home Assistant) reads a room's stream at a
bounded rate from `GET /api/visualizer?zone=<room>` ("The HTTP stream" below,
`docs/decisions/0153-the-visualizer-stream-over-http.md`).

Every value below names its source or says ASSUMED. Nothing here is a timing claim about a
light: the end-to-end tests compare stamps with stamps, and an HTTP frame's `lead_ms` with
arrival times, on one machine over loopback, and no report in `docs/measurements/` measures a
light against a sound yet.

## Where it is computed

`crates/dsp/src/visualizer.rs` (`Analyzer`, `Frame`, `Colour`) is a pure library: no clock, no
I/O, no allocation after `Analyzer::new`, the same frames for the same samples however they are
split. The server runs one analyser per stream slot on the audio thread
(`crates/server/src/slots.rs`), after each tick's broadcast and outside the grid guard, over the
chunk that slot just played (the configured stream, a chime, a line-in, or silence). A slot no
visualizer session is on, and no room an HTTP subscriber watches, is not analysed
(`Router::watched`, an atomic per fanout and one per slot in `crates/server/src/lights.rs`); a
slot that gains one is analysed from scratch from that tick. The one-stream shape (`--slots 0`) sends no
visualizer stream (a follow-up).

## What a frame carries

| Field | Value | Source |
|---|---|---|
| Analysis hop | 10 ms (100 Hz) | Dixon 2006, section 2: "calculated at a frame rate of 100 Hz" |
| Window | Hamming, the power of two nearest 46.4 ms (2048 at 44.1 and 48 kHz) | Dixon 2006, section 2: N = 2048 at 44.1 kHz, Hamming |
| Frame rate | 25 a second (4 hops) | WLED's audio sync sends 50 ("one packet every 20 milliseconds", https://mm.kno.wled.ge/soundreactive/sync/); halved, ASSUMED, so frames up to the wireless tier's 500 ms early fit the status LED's 16-event queue |
| `peak` | sample peak over the frame, every channel, dBFS, instant attack, falling 24 dB in 2.8 s | EBU Tech 3205-E (1979) PPM return time, https://tech.ebu.ch/docs/tech/tech3205.pdf; its use here ASSUMED |
| Level byte | `clamp(round(255 (dBFS + 60) / 60), 0, 255)` | ASSUMED (60 dB span) |
| Bands | 60 sixth-octave bands, 21.1 Hz to 18.8 kHz, `fm = 1000 G^((2x+1)/12)`, `G = 10^(3/10)`, `x = -34..25`, power averaged over the frame, then the PPM fall | IEC 61260-1:2014 clauses 3.11, 5.2.1, 5.3, 5.4.2 (preview pages); the fall ASSUMED for bands |
| Bands per endpoint | its `visualizer_bands`, at most 60 (64 asked gets 60); output band `j` sums the power of bands `j*60/n` to `(j+1)*60/n` (30 asked gives the third-octave bands) | IEC 61260-1 5.4 Note 1 (bands "can be combined") |
| `beat` | 0, or `min(255, round(64 z))` where `z` is the onset hop's flux in running deviations above the running mean | ASSUMED scale (the research's); 128 and up flashes the status LED (two deviations) |
| Stamp | the frame's first hop, or the onset hop when it carries a beat, plus the session's heard latency | below |

### The beat

Spectral flux, `SF(n) = sum_k H(|X(n,k)| - |X(n-1,k)|)`, linear magnitudes, L1 (Dixon,
"Onset Detection Revisited", DAFx-06, section 2.1,
https://www.dafx.de/paper-archive/2006/papers/p_133.pdf). Peak picking (section 2.6): the hop's
flux is the largest within `w = 3` hops either side, and its normalised value is at least the
mean over `m w = 9` hops before to `w` after plus `delta`. Dixon's third condition (`g_alpha`)
is left out; he found its improvement "marginal, assuming a suitable value for delta is chosen".
Dixon normalises the whole function to mean 0 and deviation 1; a stream has no whole, so the mean
and deviation are exponential running ones over 3 s (ASSUMED; the EBU short-term window, as the
research suggests), every hop in the picking window normalised by the same ones, with the
deviation and the flux floored at -60 dB of a full-scale sine's bin (ASSUMED) so silence never
beats. `delta` = 0.5 (ASSUMED: the paper prints no value). The look-ahead is 3 hops (30 ms) plus
half a window (23 ms): a frame leaves the server about 91 ms after the audio it describes was
cut, and is heard 180 ms (wired) or 500 ms (wireless) after it was cut.

### The colour

Hue from the spectral centroid, blue at 100 Hz to red at 8 kHz, log in between; saturation one
minus the spectral flatness; brightness the level byte of the band range's power. The direction
(centroid blue to red, flatness to saturation) is Richan and Rouat, "A proposal and evaluation of
new timbre visualisation methods for audio sample browsers", 2020, section 4
(https://arxiv.org/pdf/2011.15096); they found colour had "little effect" on task performance, so
this is an aesthetic choice with precedent. The 100 Hz and 8 kHz ends are ASSUMED. All three are
smoothed with a first-order 0.4 s time constant (ITU-R BS.1771's momentary meter as EBU Tech 3341
V4 reports it, https://tech.ebu.ch/docs/tech/tech3341.pdf), unweighted (no K-weighting: ASSUMED,
a follow-up). RGB by Smith's hexcone HSV (SIGGRAPH 1978, as at
https://en.wikipedia.org/wiki/HSL_and_HSV#HSV_to_RGB).

A `color` goes out at most every 500 ms with `transition_ms` = 500, and only when the hue moved
15 degrees, the saturation 0.1 or the brightness 16 since the last one (all ASSUMED). 500 ms keeps
it inside WCAG 2.2 success criterion 2.3.1's "three flashes in any one second period" (a screen
rule applied to lamps, ASSUMED transfer) and below the smart lights' command rates the research
found (Nanoleaf 10 Hz, Hue effects under 12.5 Hz; both LEADs). A session is sent a colour only
when it differs from the last one it was sent.

## When a frame is heard

A chunk stamped `t` is heard at `t + playout latency` on the server timeline at every endpoint
of a group (`crates/client-linux/src/sync.rs`). The server stamps a frame for each session at
the instant the analysis names (`origin + sample * 1e9 / rate` on the slots' grid) plus that
session's heard latency: its room's tier (`crate::conductor::heard_latency_ns`): 180 ms wired
(`config/sync.conf` `playout_latency_us`, held equal by a test), the wireless policy's 500 ms
wireless. The conductor keeps it current for every visualizer session on every pass; until its
first pass a session is taken as wired.

## Who receives it

Every session the router carries on the slot's fanout that declared the `visualizer` role: the
endpoints of the room or group the slot plays. A player without the role receives nothing; a
session in no room is on the silent fanout, which is never analysed, and receives nothing. A run
of silent frames sends its first and then nothing until there is something to show. A message a
full session queue refuses is dropped and counted (`VisualizerCounts`), never retried: the next
frame supersedes it.

And every subscriber of `GET /api/visualizer?zone=<room>` on the HTTP control plane, for the
room it names: no session, no role, no endpoint identity. It is sent the same analysis under
the rules of the next section.

## The HTTP stream

`GET /api/visualizer?zone=<room id>` on the control listener is a server-sent event stream
(`docs/control-plane.md`, "How the messages travel"): the response headers, one comment line
(`: visualizer`), then one `data:` line per frame, each one catalog version 2 message in the
canonical encoding (`fixtures/visualizer/http-frame.json`; `http-frame.sse` is the event's
bytes on the stream):

```json
{"v":2,"t":"visualizer","zone":"den","timestamp_ns":12345678901,"lead_ms":85,"peak":201,"beat":255,"red":255,"green":96,"blue":0,"brightness":180,"transition_ms":500}
```

| Field | Value |
|---|---|
| `zone` | the room the stream was opened for |
| `timestamp_ns` | when that room hears the audio the frame describes, ns on the server timeline: the wire's stamp for an endpoint of that room (the frame's first hop, or the onset hop when it carries a beat, plus the room's heard latency, "When a frame is heard"). An integer; past 2^53 ns (104 days of server uptime) a reader that keeps JSON numbers as doubles loses its last digits, and `lead_ms` is what such a reader uses |
| `lead_ms` | `timestamp_ns` less the server timeline's now at the moment the server rendered the frame for writing, whole ms rounded down: how long after the frame was written the room hears it. A subscriber has no clock on the server timeline, so this is the field it acts on: show the frame `lead_ms` after it arrives, less its own delay to the lamp. Zero or negative when the room has heard it already |
| `peak` | the wire's `peak`: the level byte of the held sample peak, 0 to 255 |
| `beat` | the wire's `beat`: 0, or the strength of the last onset this subscriber has not been sent yet (128 and up is two deviations) |
| `red`, `green`, `blue`, `brightness`, `transition_ms` | the colour in force: the last `color` the analysis made for that slot ("The colour"), carried by every frame; all 0 until the first |

There are no bands: a lamp has one colour and one level. An endpoint that wants a spectrum
declares the `visualizer` role on the audio wire.

**The room.** `zone` names a room; a room this server does not have is answered `404`, a
request with no `zone` `400`, and neither holds anything. A group's stream is read through any
of its rooms: every room of a group is on the group's slot, so the frames are the same and
only the stamp is the room's own tier's. The subscription follows the room: when the room
joins or leaves a group the conductor's next pass moves it to that group's slot. A room whose
group plays nothing (source `none`: no slot) is sent nothing, and so is every room of a server
in the one-stream shape (`--slots 0`), which computes no visualizer stream: the stream opens,
is kept alive and stays empty.

**The rate cap.** A subscriber is sent at most one frame every 100 ms, so at most 10 a second
(`MIN_FRAME_INTERVAL`, `crates/server/src/lights.rs`). Cited, not measured: the smart lights'
command rates the research found are Nanoleaf's "no faster than 10Hz" and Hue's effect rate
under 12.5 Hz (`docs/research/research-dsp-phase-b.md` section 2, both LEADs), and 10 a second
is inside both. The interval is taken on the event writer's monotonic clock between the
moments it renders two frames for that subscriber; the writer wakes itself when a held-back
subscriber's interval runs out, so a frame waits for the cap and not for the next frame.

**The drop rule: superseded, never queued.** The server keeps ONE frame per subscribed room,
the latest; the audio thread overwrites it 25 times a second and a subscriber is sent
whichever is there when it may next be sent one. So a subscriber that is slow, or held by the
cap, gets the latest frame and never a backlog, and it is never dropped for being slow (there
is no queue to fill; only the event writer's 5 s stall bound ends its stream). Two things are
carried past the frame that made them, because a lamp needs them and no later frame repeats
them: the colour in force, and the last beat, which rides in the next frame the subscriber is
sent, with `timestamp_ns` the beat's own (its onset's), unless a later beat replaced it first.
Frames a subscriber was not sent are counted (`light_superseded` in `GET /api/report`, beside
`light_subscribers` and `light_frames`). As on the wire, a run of silent frames sends its
first and then nothing until there is something to show, and a stream sends only frames made
after it was opened.

**What `lead_ms` is worth.** The analysis leaves a frame about 91 ms after its audio was cut
("The beat") and the cap can hold it up to 60 ms more (a frame every 40 ms against an
interval of 100 ms), so in a wired room (heard 180 ms after the cut) a beat reaches the
subscriber a few tens of ms before it is heard, and in a wireless room (500 ms) about 320 ms
more. Measured by `crates/server/tests/visualizer_stream.rs` on 2026-10-04, on one machine
over loopback with an unoptimised build: the four kicks' beat frames carried `lead_ms` 34 to
35 in the wired room and 350 to 351 in the wireless one, every frame's between 34 and 95
(wired) and 350 and 411 (wireless), and the time from a frame's arrival to its room hearing
it, on the test's own clock, was 2.1 to 3.1 ms more than its `lead_ms` said (53 frames a
room; the difference is the audio chunks' own delay to the test, which is how the test finds
the server's clock). That is the server's arithmetic checked against a clock, on loopback; it
is not a measurement of a network, of Home Assistant or of a lamp, each of which takes its
own share of the lead, and a wired room's 35 ms may not cover them. A light that must land
on the beat in a wired room needs the report the last follow-up below asks for.

## The endpoints

- **The Linux client.** `--visualizer-bands <n>` (1 to 64) declares the role, asks for `n` bands
  and logs a `visualizer beat ...` line per frame with a beat and a `visualizer color ...` line
  per colour. A front panel whose light follows the visualizer (`docs/decisions/0067-*`)
  declares the role itself; its LED now shows frames against the SERVER timeline (the endpoint's
  monotonic now plus the sync offset the running session's playout loop publishes), which was
  goal 10's follow-up.
- **The C endpoint.** Its status LED already takes 0x34 and 0x35 on the server timeline (ADR
  0063, `fixtures/controls/visualizer-sequence.hex`); nothing changed there.

## Fixtures and tests

`fixtures/visualizer/` (Rust only by declaration: the server computes the stream, no endpoint
does): a 120 BPM kick pattern, a 1 kHz tone at -6.02 dBFS, a 50 Hz to 6 kHz sweep and silence,
each a mono 16-bit WAV beside the `.params` it is regenerated from byte for byte, with its
expectation. `crates/dsp/tests/visualizer_fixtures.rs` holds both; on the fixture's own hop grid
the four kicks beat at +10, 0, 0, 0 ms of their onsets (tolerance 20 ms).
`crates/server/tests/visualizer_stream.rs` writes the kick fixture into a named pipe the real
server plays and checks the frames a visualizer endpoint receives: one beat per kick, within the
fixture's tolerance plus one hop (30 ms) of the kick on the server timeline as the den hears it,
16 bands each, a colour; the same beats 500 ms after their kicks for a visualizer endpoint in a
wireless room playing the same stream on the other slot; and nothing at all for a player without
the role or an endpoint in no room.

The same file's second test reads the HTTP stream with no visualizer session at all: three
raw-socket subscribers of `GET /api/visualizer`, for the den, the wireless study and an attic
whose group has no slot. The den and the study each receive one beat per kick, stamped within
the same tolerance of when that room hears the kick, each beat frame with a colour; the attic
receives nothing; no two frames were written closer than the cap (by `timestamp_ns` less
`lead_ms`) and no more arrive than the cap allows; and `lead_ms` agrees with the time to
hearing measured on the test's own clock (the offset between the two clocks is the least
arrival-less-stamp over the den's audio chunks, which the server never sends before their
stamp). `fixtures/visualizer/http-frame.{fields,json,sse}` pin one frame's bytes
(`crates/server/src/lights.rs`, `the_committed_frame_is_these_bytes`).

## Follow-ups

- The one-stream shape (`--slots 0`) sends no visualizer stream.
- Brightness is unweighted; K-weighting through `crates/dsp`'s biquads (BS.1770) when wanted.
- The Home Assistant integration's lights reading `GET /api/visualizer` ("The HTTP stream";
  the server's half is done, the integration's entity is its own task).
- No measurement of a light against a sound: a bench report would say how far an LED's flash is
  from the kick a microphone hears.
