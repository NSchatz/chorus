# The DSP library

chorus's sound processing (goal 12): `crates/dsp` (package `chorus-dsp`) and its C mirror
`firmware/include/chorus/dsp.h` + `firmware/src/dsp.c`. The same blocks, the same algorithms and
the same state-update order in both languages, held to one set of committed fixtures under
`fixtures/dsp/` (`crates/dsp/tests/shared_fixtures.rs`, `firmware/tests/test_dsp.c`;
`fixtures/README.md` "dsp/"). Pure libraries: no clock, no I/O, and on the C side no heap.

The decisions: `docs/decisions/` "the DSP library". The research behind the phase B values:
`docs/research/research-dsp-phase-b.md` and [`.claude/goals/2026-09-chorus-research/research-theater.md`](https://github.com/NSchatz/chorus/blob/535ed2816b5735153ac81c52a235024aa806f2b5/.claude/goals/2026-09-chorus-research/research-theater.md)
section 5.

## Numbers

Samples are `f32`. Coefficients are designed in `f64` and rounded to `f32` once (BRIEF section
5.6). Rust never fuses `a * b + c`; the C unit is compiled with `-ffp-contract=off
-fno-fast-math` and writes every float operation with the `f` suffix, so one float operation is
one rounding in both languages. On the host the two chains agree to within 1.5e-8 on every
golden sample (the fixtures allow 1e-6).

A consequence of `f32` coefficients worth knowing: a section whose corner is far below the rate
has its poles close to `z = 1`, and rounding its coefficients moves its low-frequency gain
slightly. The LR4 low-pass at 80 Hz of 48 kHz is 0.005 dB low at 40 Hz (the 2.1 subwoofer
fixture says so and allows for it).

## The blocks

1. **Biquad** (`biquad`, `chorus_dsp_biquad_*`): the RBJ Audio EQ Cookbook's eight designs
   (lowpass, highpass, bandpass with constant 0 dB peak, notch, allpass, peaking, low shelf,
   high shelf), normalised by `a0`, run as Transposed Direct Form II
   (`y = b0 x + z1; z1 = b1 x - a1 y + z2; z2 = b2 x - a2 y`). `magnitude_db(f)` and the complex
   `response(f)` evaluate a design in `f64` (fixtures, room fitting). Changing coefficients keeps
   the state.
2. **LR4 crossover** (`crossover`, `chorus_dsp_lr4_*`): each branch two identical Butterworth
   sections (Q = 1/sqrt 2) at the crossover; the high branch is not inverted. `split` gives
   `(low, high)`; their sum is an all-pass.
3. **Delay** (`delay`, `chorus_dsp_delay_*`): whole frames, at most 4800 (50 ms at 96 kHz), refused
   above. Microseconds become frames by `(us * rate + 500000) / 1000000` in integers. The C line
   runs over caller storage.
4. **Look-ahead limiter** (`limiter`, `chorus_dsp_limiter_*`): channel-linked. The signal is
   delayed by the look-ahead `L`; each frame's required gain is `ceiling / peak` when its peak
   exceeds the ceiling, else 1; the gain is the least of a release candidate, the required gain of
   the frame leaving the delay, and for every later frame needing less than the current gain the
   linear ramp that reaches it as it plays; then the output is clamped to the ceiling. So no
   sample ever exceeds the ceiling, and input that never exceeds it comes out delayed, bit for
   bit. The release runs on the distance below unity as its own state and snaps to exactly 1.0
   below 1e-6, so a chain that has limited becomes bit-exact again.
5. **Night compressor** (`compressor`, `chorus_dsp_compressor_*`): feed-forward, the detector in the
   log domain after the gain computer (Giannoulis, Massberg and Reiss 2012's recommendation), the
   soft-knee static curve, attack and release smoothing of the computed gain with
   `a = exp(-ln 9 / (Fs T))` (T is the 10-90 % time), a makeup gain, linked over all channels.
6. **Loudness compensation** (`loudness`, `chorus_dsp_iso226_*`, `chorus_dsp_loudness_*`): ISO
   226:2003's table and formula give `Lp(f, phon)`. Playing `att` dB below the reference level,
   at `L = 80 - att` phon (held to 20..80), the boost a frequency needs to keep its balance with
   1 kHz is `(Lp(f, L) - Lp(1k, L)) - (Lp(f, 80) - Lp(1k, 80))`: taken at 50 Hz for a low shelf
   at 100 Hz (capped at +12 dB) and at 10 kHz for a high shelf at 8 kHz (capped at +6 dB), never
   negative. The attenuation comes from the room gain, `-20 log10(gain)`, in 0.5 dB steps, so a
   volume ramp redesigns the shelves at most once per step and never resets them. At unity both
   gains are exactly 0 and the stage is off.
7. **Speech enhancement** (`speech`, in the chain): a peaking boost at 2 kHz, Q 0.667, +4 dB on FC
   when the stream has one; with FL and FR, on the mid of a mid/side split
   (`m = (l + r)/2`, `s = (l - r)/2`, `l = m' + s`, `r = m' - s`); on the one channel of a mono
   stream.
8. **Bass management** (in the chain): by the endpoint's role in its bonded set. A main role, with
   a subwoofer in the set, plays its channel through the LR4 high branch at `crossover_hz`. The
   `LFE` role plays the LR4 low branch of the sum of the stream's main channels, plus the
   stream's LFE channel at +10 dB, times the subwoofer level, inverted if set. Every bonded
   endpoint has the room's whole stream, so each computes its own feed.
9. **Two-way** (in the chain, `EndpointDsp`/`chorus_dsp_endpoint_t`): the endpoint's own drivers,
   not the catalog's. Its one input (the role's channel, or unbonded the downmix `(FL + FR)/2`,
   the mono channel, or the mean of the main channels) is split by LR4 at its `crossover_hz` into
   a woofer (output 0) and a tweeter (output 1), each with a trim (cut only, to -24 dB), a delay
   and a polarity.
10. **The chain** (`Chain`, `chorus_dsp_chain_t`): configured from a `SoundSettings` (exactly the
    wire `sound` message's fields: `bass_db`, `treble_db`, `loudness`, `night`, `speech`,
    `room_eq_enabled`, `sub_polarity_inverted`, `role`, `sub_present`, `crossover_hz`,
    `sub_level_cdb`, up to 8 room-EQ filters `{freq_hz, gain_cdb, q_milli}`), the endpoint's
    `EndpointDsp`, the stream's channel map and rate. It processes interleaved frames given the
    room gain and the room's effective limit gain.

## The chain's order

Per frame:

1. room EQ (peaking filters, every channel but LFE, when enabled; a 0 dB filter is skipped);
2. tone: the bass low shelf and the treble high shelf (every channel but LFE);
3. loudness: the two ISO 226 shelves (every channel but LFE);
4. speech;
5. night;
6. bass management and the output's source, by role (role 0 and no two-way: every stream channel
   to its own output);
7. the two-way split;
8. each output's delay (`output_delay_us[o]`, plus the driver's `delay_us` when two-way);
9. times the room gain;
10. the look-ahead limiter at `min(1, limit gain)` (K81, I10: no DSP boost lifts a room above its
    limit).

Outputs: the stream's channel count (role 0, no two-way), 2 (two-way), else 1. The latency is
the limiter's look-ahead whatever the settings (2 ms: 96 frames at 48 kHz), so a setting never
moves the audio in time. A **flat** chain (the default settings, role 0, no two-way, no delays)
runs no filter: each output is its input times the room gain, delayed by the look-ahead, bit for
bit (`fixtures/dsp/chain-flat-*.txt`).

`set_sound` / `chorus_dsp_chain_set_sound` can be called on every `sound` message: a filter whose
gain changes keeps its state; a stage that switches on starts from zero state; a change of the
output layout (role, subwoofer presence, crossover, delays) resets the output side. A refused
setting changes nothing. A new stream (rate or channel map) or new endpoint configuration is a
new chain.

Fixed maximums (both languages): 8 stream channels, 8 outputs, 4800 delay frames per output and
9600 for all outputs together, a 768-frame look-ahead. A `chorus_dsp_chain_t` is 72872 bytes (goal 13's mix rows and ambient filters included),
one static object; nothing on the audio path allocates.

## The theater maps (goal 13)

What step 6 plays when the stream and the room's set do not match: a stereo TV into a 5.1 set, a
5.1 stream into a stereo pair or one speaker. The same rules in both languages
(`crates/dsp/src/chain.rs`, `firmware/src/dsp.c`); each row is a shared fixture
(`fixtures/dsp/chain-theater-*.txt`, read by `crates/dsp/tests/shared_fixtures.rs` and
`firmware/tests/test_dsp.c`), its expected value computed from the cited coefficient in
`tools/dsp-fixtures/generate.py`, and its samples held to the Rust chain's.

| the endpoint | the stream | it plays | source |
|---|---|---|---|
| a role the stream carries | any | that channel (as goal 12) | |
| FL or FR, the set has no FC (`fold_centre`) | has FC | `+ FC / sqrt 2` | BS.775-4 Table 2, 2/0 and 2/2 |
| FL or FR, the set has no surrounds (`fold_surround`) | has its side's surround(s) | `+ BL / sqrt 2 + SL / sqrt 2` (FR: BR, SR) | BS.775-4 Table 2, 2/0 and 3/0 |
| FC | no FC, has FL and FR | `(FL + FR) / sqrt 2` | the passive matrix's centre [DS] |
| SL, SR, BL or BR | lacks it, has the other naming (BL for SL) | that channel | 5.1's two namings [MS] |
| SL, SR, BL or BR | no surround at all, `tv_upmix` off | silence | ASSUMED default |
| SL, SR, BL or BR | no surround at all, `tv_upmix` ambient | `(FL - FR) / sqrt 2`, band 100 Hz..7 kHz, 20 ms late | the passive matrix's surround [DS] [SOS] |
| LFE | any | as goal 12 (low branch of the mains + LFE at +10 dB) | ATSC A/52 |
| not in a set, `stereo_downmix`, not two-way | has FC or a surround | two outputs, `Lo = FL + FC / sqrt 2 + LS / sqrt 2`, `Ro` likewise; LFE dropped | BS.775-4 Table 2, 2/0; Annex 7 |
| not in a set, two-way | has FC or a surround | `(Lo + Ro) / 2` into the split | BS.775-4 Table 2, the chain's own mean |
| a mono stream | `MONO` | its one channel (as goal 12) | |

- **The coefficient.** BS.775-4 prints 0.7071; the chain uses 1/sqrt 2 exactly (designed in f64,
  rounded to f32 once). Every term is summed in a fixed order (the role's own channel, FC, then
  the back and side surrounds), so both languages agree bit for bit.
- **The centre on a stereo stream** is the passive Dolby Surround decoder's: its encoder puts
  the centre into Lt and Rt "divided equally ... with a 3 dB level reduction", so
  `(Lt + Rt) / sqrt 2` gives a centre-panned sound back at unity (computed). The front members
  keep FL and FR whole, as the envelope asks (no "phantom" subtraction).
- **The ambient surround** is the same decoder's surround: "the difference of Lt and Rt, then
  ... a 7 kHz low-pass filter, a delay line" [DS]; a band-pass "below 100Hz and above 7kHz" and
  the surrounds "about 20mS after the direct sound from the front channels" [SOS]. Both
  surround members play the same mono surround (the matrix carries one). The band is one RBJ
  high-pass at 100 Hz and one RBJ low-pass at 7 kHz, each Q 1/sqrt 2 (ASSUMED: the sources give
  the corners, not the slopes); the 20 ms rides the output's own delay line, so it adds to
  `output_delay_us` and a chain where it does not fit (above 4800 frames, so at 384 kHz) is
  refused as any too-long delay is. This is the one place a setting moves an output in time, on
  purpose (the precedence effect keeps the image on the screen), and only on a surround member
  that has nothing of its own to play; the chain's reported latency stays the look-ahead, so no
  endpoint's sync compensates it away. A centred sound (FL = FR) cancels exactly and leaves the
  surrounds silent (`chain-theater-2p0-ambient-centre.txt`).
- **The LFE in a downmix** is dropped: "The LFE channel is often not included in a 2-channel
  downmix" (BS.775-4, Annex 7). A stereo pair with a sub keeps the sub's feed as goal 12 has it.
- **Where the facts come from.** `fold_centre`, `fold_surround` and `tv_upmix` arrive in the
  `sound` message's theater block (`docs/protocol.md`, 0x39): the server tells a front member
  what its set lacks. `stereo_downmix` is the endpoint's own (`EndpointDsp`,
  `chorus_dsp_endpoint_t`): whether it is one stereo speaker. The Linux client sets it for a
  device of two channels with no output map and no two-way when the stream has more than two
  (`crates/client-linux/src/dsp.rs`, `stereo_fold`; ADR 0092): the chain then engages from the
  first frame, `sound` or not, and says `dsp stereo-downmix stream_channels=<n>
  device_channels=2 ...`. A device that refuses a surround stream's channel count is opened
  with two (`device-channels ... reason=stream-count-refused`); one that accepts it (ALSA's
  plug layer) is fed the stream's channels as before. The firmware is 2-channel and is refused
  a 5.1 stream at negotiation today, so nothing sets it there yet.
- **Positions past 5.1** (FLC, FRC, BC, the tops) are not folded: they are silent on an endpoint
  that does not play them, as before.

## The announcement mixer

`crates/dsp/src/duck.rs` (`chorus_dsp::duck`): the music ramps down to a duck gain, a clip mixes
over it, and the music ramps back. It is not a stage of the chain above and has no C mirror: it
runs on the server, on a room's stream before it is encoded, so every endpoint of the room plays
one mixed stream in sync and the endpoint's chain (and its limiter) is unchanged
(`docs/decisions/` "the announcement mixer"). The library is the core only. The server calls it
from its audio thread, once a tick for each announcement that is playing
(`crates/server/src/mixer.rs`, `docs/control-plane.md` "Announcements", and the record
"an announcement is mixed over its rooms" in `docs/decisions/`); a room whose source is a Soloist
instance is paused for an announcement, never mixed: `docs/soloist.md`.

Pure, as the rest: it reads no clock. Every length is a count of frames and only a frame passing
through `Duck::process` moves its state, so an event lands on a frame by ending the block there.

**The envelope.** One integer `level` runs from 0 (untouched) to `D x R` (fully ducked), `D` the
duck ramp and `R` the restore ramp in frames. A ducking frame adds `R`, a restoring frame takes
`D` away. With `p = level / (D x R)`:

- the music's gain is `g = 1 - (1 - duck_gain) p`, exactly `duck_gain` at `p = 1`;
- the clip's gain is `clip_gain` while the music is held down, and `clip_gain x p` for a
  cancelled clip, which fades out with the restore instead of being cut;
- the output is `music x g + clip x w`, clamped to `-limit..=limit`;
- at `level = 0` the frame is copied, not multiplied.

`p` is computed in `f64` from the integers and each gain rounded to `f32` once. What follows
from it, each held by `crates/dsp/tests/duck_fixtures.rs` against `fixtures/dsp/duck/`:

- **Down in `D` frames, to the frame.** After `start`, frame `n` (from 0) is at
  `1 - (1 - duck_gain)(n + 1) / D`; frame `D - 1` is the first at the duck gain.
- **The clip starts when the ramp ends**: its first frame is frame `D`, and no clip frame is
  taken on the way down ("The start of the notification playback is synchronized with the end of
  the ducking ramp", Android [AF]). While the music is held, each frame takes one clip frame; with
  none to take (a decoder that is late) the music stays ducked under silence.
- **Back in `R` frames, bit for bit.** `finish` says the clip has no more frames than those
  handed in; the frame after its last begins the restore, and the `R`th frame after it, and every
  frame from there, is the music's own sample (a copy: a negative zero stays one).
- **A clip that ends early** (ten frames, or none) restores the same way.
- **A cancel** begins the restore on the next frame from the gain the music is at: half way down
  it is back in `R / 2` frames, with no jump and no clip frame played; while the clip plays, the
  clip fades over the `R` frames. A `start` during a restore ducks again from where it is.
- **Click-free.** The music's gain moves by at most `(1 - duck_gain) / min(D, R)` per frame
  (`DuckParams::max_gain_step`), so a steady music's output moves by no more than that times its
  level; a cancelled clip's gain moves by at most `clip_gain / R` per frame.
- **Never above the limit.** `duck_gain + clip_gain <= 1` is checked when the parameters are
  made (refused otherwise), and `g + w <= 1` on every frame, so a music and a clip inside the
  limit mix to a sum inside it; the clamp takes the last rounding. Frames the mixer does not
  touch are not clamped: they are the music's, and the chain's limiter holds the room to its
  ceiling as before.

**Parameters** (`DuckParams`; `DuckParams::from_ms` makes the frame counts from milliseconds and a
rate by `(ms x rate + 500) / 1000` in integers, at least one frame):

| Parameter | Range | Default | Source |
|---|---|---|---|
| `duck_gain` (the depth) | 0..=1 linear; -50..=0 dB in `from_ms` | -20 dB (0.1) | cited: the ESPHome mixer speaker's `apply_ducking` example, `decibel_reduction: 20`; its range "between 0 and 50" [EM]. The Audacity manual's Auto Duck default is -12 dB [AD]: the deeper of the two is taken because the clip is speech over music |
| `clip_gain` | 0..=`1 - duck_gain` | `1 - duck_gain` (0.9, -0.9 dB), rounded down | computed: the most the limit bound leaves |
| `duck_ramp_frames` | 1..=3 840 000 (10 s at 384 kHz) | 200 ms (9600 frames at 48 kHz) | ASSUMED: not measured; shorter than the restore because the clip waits for it. The sources' fades are 0.5 s [AD] and a 2 s example [EM], neither for a clip that waits |
| `restore_ramp_frames` | 1..=3 840 000 | 500 ms (24000 frames at 48 kHz) | cited: Auto Duck's fade up length, "default: 0.5 seconds" [AD] |
| `limit` | (0, 1] | 1.0, full scale | the stream's own range; the room's limit stays the chain limiter's (K81, I10) |

A stream of 1 to 8 channels; the clip has the music's channel count and rate (making it so is
the caller's). How the defaults sound is unmeasured: they are starting points until a room is
listened to.

## Design records

`crates/dsp/src/design_record.rs` (`chorus_dsp::design_record`) reads a **speaker design
record**: the plain JSON file that the acoustics package of the owner's shared Python library
exports (BRIEF section 8, "Acoustic design tools"). The record is the whole seam between that
library and chorus: chorus imports nothing from it and reads the file with its own reader.
Nothing runs a record in a chain yet; what is proven is that chorus can read one and that its
crossover, run through chorus's own biquads, gives the response the record states.

The schema is `speaker-design-record`, **version 1**. A record holds a name, a `tolerance_db`
and one or more crossovers; a crossover holds its kind (`lr4`, two sections per branch, or
`lr2`, one), the sample rate its coefficients are for, the crossover frequency, each branch's
biquads as `b0 b1 b2 a1 a2` with `a0 = 1` (the form `biquad::Coefficients` holds), and response
points: the level in dB of the low branch, of the high branch and of the two added, at chosen
frequencies. `DesignRecord::parse` refuses text that is not JSON, any other schema, any other
version (one it does not know is never guessed at), a key version 1 does not have, a value out
of range, and biquads that do not give the record's own response points within its
`tolerance_db`, so a record edited by hand is not run. `Crossover::lr4_design` gives an `lr4`
as the `Lr4Design` the chain's crossover runs.

The fixture is `fixtures/design-record/lr4-2000hz-48k.json`, exported from library release
v1.49.0 on 2026-10-06, with `lr4-2000hz-48k.provenance` beside it (the release tag, the export
command, the date and the file's sha256). `crates/dsp/tests/design_record.rs` holds the file to
that sha256, reads it, and checks:

- the designed response (`f64`) at every response point, to the record's `tolerance_db`
  (1e-6 dB);
- the running `f32` filters: a sine at each response frequency through both branches, two
  seconds played and the last one measured. Each branch's level where the record puts it above
  -40 dB (the crossover frequency always among them: -6.0206 dB each for an LR4) and the level
  of the two added (0 dB at every point: the flat sum) are within **0.001 dB** of the record.
  Measured 2026-10-06: at most 0.000027 dB off. Below -40 dB a single-precision filter's own
  rounding is a visible part of its output, so those levels are held by the designed response
  alone;
- that the record's sections are the ones `Lr4Design::new` designs for the same rate and
  frequency (to 1e-12), and that `Lr4` built from the record gives the cascade's samples bit
  for bit.

Reading the JSON: `chorus-dsp` has no dependency, and the record's shape is fixed and small, so
the module carries its own reader of the JSON a record uses (RFC 8259,
<https://www.rfc-editor.org/rfc/rfc8259>, read 2026-10-06) instead of taking a JSON crate as a
dependency for one file format. The test computes the sha256 itself (FIPS 180-4,
<https://csrc.nist.gov/pubs/fips/180-4/upd1/final>, read 2026-10-06) for the same reason.

A second record is a real design: `fixtures/design-record/chorus-compact-v1.json`, the compact
speaker's LR4 at 3500 Hz for 48 kHz (`docs/hardware/compact-speaker.md`, which also lists that
speaker's EQ sections in the same `b0 b1 b2 a1 a2` form). The same test reads it with every
other file, and `the_compact_record_is_the_exported_one_and_runs` holds it by name to its
sha256 and to its crossover's levels: measured 2026-10-06, at most 0.000006 dB off.

A third is the active two-way's: `fixtures/design-record/chorus-twoway-v1.json`, an LR4 at
2000 Hz for 48 kHz (`docs/hardware/twoway-speaker.md`, with that speaker's EQ sections in the
same form). `the_twoway_record_is_the_exported_one_and_runs` holds it by name to its sha256
and to its crossover's levels: measured 2026-10-06, at most 0.000027 dB off.

A fourth is the subwoofer's: `fixtures/design-record/chorus-sub-v1.json`, the bass-management
LR4 of chain item 8 at the default `crossover_hz`, 80 Hz for 48 kHz
(`docs/hardware/subwoofer.md`, with that speaker's protective high-pass in the same form, its
amplifier sizing and what its level and phase knobs set).
`the_sub_record_is_the_exported_one_and_runs` holds it by name to its sha256 and to its
crossover's levels. This low in the band the running filters' `f32` coefficients are a visible
part of the difference: rounding them moves the designed response by up to 0.006324 dB (at
20 Hz; 0.000023 dB for the two-way's 2000 Hz), so the test takes that rounding out of every
record's running check before it holds the filters to 0.001 dB (measured 2026-10-06: 0.000990 dB
for this record, 0.000011 dB for the two-way's) and holds the rounding itself under 0.02 dB.

To refresh a fixture: `fixtures/README.md`, "`design-record/`".

## On the endpoints

Both endpoints run the chain on every frame they write (the decisions: `docs/decisions/` "the DSP
chain on the endpoints"). The C endpoint runs it in the playout path's writer
(`firmware/src/endpoint_dsp.c`, called from `chorus_playout_fill` where `room_volume`'s gain was
applied), the Linux client at the sink's edge (`crates/client-linux/src/dsp.rs`, `DspSink`, which
also resolves the output map after the chain). The same rules on both:

- **When.** The chain engages once the endpoint knows its stream and has received a `sound` or
  has a two-way. The server sends `sound` in its greeting, before any audio, so the chain is in
  the path from a stream's first frame. Before that the endpoint plays exactly as before goal 12.
  A new stream builds a new chain; each `sound` is `set_sound` (a refused one keeps the settings
  in force and is counted).
- **The gain.** The room's gain goes into the chain (the order above: after the sound stages,
  before the limiter), not onto the samples before it. The ramp is taken in blocks of 32 frames
  (ASSUMED), each at its first frame's gain. The limiter's limit is the least of the room's limit
  and the endpoint's own ceiling (`max_volume`, `--max-volume`).
- **Time.** The chain's latency (the 2 ms look-ahead) is added to the device delay each
  endpoint's sync loop reads, so a frame is written that much earlier and heard at the sync
  target; the C endpoint shifts its GPIO marker by the same frames.
- **Outputs.** One chain output (a bonded main, the subwoofer) goes to every device channel when
  no output map says otherwise; a two-way's woofer and tweeter go to the outputs its
  configuration names (C: `two_way_woofer_slot`, `two_way_tweeter_slot` in
  `firmware/config/endpoint.conf`; Linux: `--two-way crossover-hz=..,woofer=..,tweeter=..`). On
  Linux an output map is resolved against the chain's outputs: the stream's positions when the
  endpoint is in no set, else its role, so `--output 0=LFE` takes a subwoofer's feed from a
  stereo stream.
- **The subwoofer's knobs** (C, ADR 0063): the local level (a cut, -12..0 dB) adds to the
  catalog's `sub_level_cdb`, held to -12..+6 dB, and a phase knob at 90 degrees or more inverts
  the polarity the catalog sets (ASSUMED until a variable-phase all-pass is designed).

End to end on fakes, both endpoint kinds against the real server (done-when line C):
`cargo test -p chorus-server --test dsp_end_to_end` (the Linux client) and
`make -C firmware dsp-session` (the C endpoint).

What the chain costs on the ESP32-S3 is unmeasured until the owner runs bench S6
(`docs/bench-packet.md`): the endpoint console's `dsp-cost` times it in four configurations
(flat, all-on, the LFE member of a 2.1 set, the two-way) on the chip's monotonic clock and
publishes a figure only for output within tolerance of the host's checksum
(`docs/decisions/` "the DSP chain's cost on the chip"). No headroom claim is made before that
report exists.

## Defaults and ASSUMED values

Every value below that is not cited is ASSUMED: chorus's choice, not measured, and marked so in
the code.

| What | Value | Basis |
|---|---|---|
| Settings default | flat: everything off, role 0, crossover 80 Hz | the design envelope (the catalog's own defaults, e.g. loudness on, are the catalog's) |
| Crossover default | 80 Hz | cited (SVS, THX standard) |
| LFE into the subwoofer | +10 dB | cited (ATSC A/52:2018) |
| Bass shelf | 100 Hz, Q 1/sqrt 2, 1 dB per step | ASSUMED |
| Treble shelf | 8 kHz, Q 1/sqrt 2, 1 dB per step | ASSUMED |
| Corner limit | every corner held to 0.45 x the rate | ASSUMED |
| Loudness reference | 80 phon at unity volume | ASSUMED |
| Loudness evaluation | 50 Hz (low), 10 kHz (high) | ASSUMED |
| Loudness shelves | 100 Hz and 8 kHz, Q 1/sqrt 2 | ASSUMED |
| Loudness caps | +12 dB low, +6 dB high | ASSUMED |
| Loudness step | 0.5 dB | ASSUMED |
| Speech centre and Q | 2 kHz, Q 0.667 (1 to 4 kHz) | ASSUMED (the voice band "about 1-4 kHz" is a snippet) |
| Speech gain | +4 dB | cited (Geiger et al. 2015's 3.8 dB, rounded) |
| Night curve | threshold -24 dBFS, 3:1, 12 dB knee, +6 dB makeup | ASSUMED (the Dolby "Film Standard" profile in the research is a follow-up) |
| Night times | attack 10 ms, release 500 ms | ASSUMED (the research's pair; Dolby gives none) |
| Limiter look-ahead | 2 ms | ASSUMED |
| Limiter release | 100 ms time constant | ASSUMED |
| Limiter release snap | 1e-6 below unity | ASSUMED |
| Unbonded two-way input | `(FL + FR)/2`, else the mono channel, else the mean of the mains | ASSUMED |
| A role the stream lacks | silence (a mono stream plays on every main role) | ASSUMED |
| Two-way example | 2 kHz | designed for the active two-way (ADR 0217, `chorus-twoway-v1`); `two_way` stays off by default in `endpoint.conf` |
| Driver trim range | -24..0 dB | ASSUMED |
| Gain block on the endpoints | 32 frames at one room gain while it ramps | ASSUMED |
| One chain output, no output map | on every device channel (both I2S slots) | ASSUMED |
| Sub phase knob | 90 degrees or more inverts the polarity | ASSUMED |
| Two-way on the endpoints | 2000 Hz, woofer output 0, tweeter output 1 | ASSUMED (the example above) |
| `tv_upmix` default | off (silence on the surrounds of a stereo stream) | ASSUMED |
| Ambient surround band slopes | 2nd order each (RBJ, Q 1/sqrt 2) | ASSUMED; corners cited |
| Folding past 5.1 (7.1 side and back both) | each at 1/sqrt 2 | ASSUMED extension of BS.775-4 |
| Announcement duck depth | -20 dB | cited (the ESPHome mixer's example) [EM] |
| Announcement duck ramp | 200 ms | ASSUMED |
| Announcement restore ramp | 500 ms | cited (Audacity's Auto Duck default) [AD] |
| Announcement clip gain | `1 - duck_gain` | computed (the limit bound) |

## Citations

All read 2026-10-01.

- RBJ Audio EQ Cookbook, W3C Working Group Note, 8 June 2021:
  <https://www.w3.org/TR/audio-eq-cookbook/>. The designs, and the stated properties the
  `rbj-*.txt` fixtures check.
- ITU-R BS.1770-5 (11/2023), Annex 1 Tables 1 and 2, the K-weighting filter's coefficients at
  48 kHz: <https://www.itu.int/dms_pubrec/itu-r/rec/bs/R-REC-BS.1770-5-202311-I!!PDF-E.pdf>.
- pyloudnorm (MIT licence, <https://github.com/csteinmetz1/pyloudnorm>), `meter.py` and
  `iirfilter.py`: the cookbook parameters that reproduce the K-weighting (high shelf 1500 Hz,
  +4 dB, Q 1/sqrt 2; high-pass 38 Hz, Q 0.5). Read, not copied.
- D. Bohn, "Linkwitz-Riley Crossovers: A Primer", RaneNote 160:
  <https://www.ranecommercial.com/legacy/note160.html>; and Linkwitz Lab, "Active Filters":
  <https://www.linkwitzlab.com/filters.htm>.
- J. O. Smith III, "Physical Audio Signal Processing", Delay Lines:
  <https://ccrma.stanford.edu/~jos/pasp/Delay_Lines.html>.
- Steinberg, "Brickwall Limiter":
  <https://www.steinberg.help/r/groove-agent/6.0/en/halion/topics/effects_reference/brickwalllimiter_r.html>
  ("the output level never exceeds a set limit").
- D. Giannoulis, M. Massberg, J. D. Reiss, "Digital Dynamic Range Compressor Design: A Tutorial
  and Analysis", JAES 60(6), 2012: <https://secure.aes.org/forum/pubs/journal/?ID=174> (the PDF
  could not be fetched); its static curve and the 10-90 % time constants as MathWorks prints
  them, citing it: <https://www.mathworks.com/help/audio/ref/compressor-system-object.html>.
- ISO 226:2003, "Acoustics: Normal equal-loudness-level contours":
  <https://www.iso.org/standard/34222.html> (sold); its parameter table and formula as reproduced
  at <https://www.dsprelated.com/showcode/174.php>.
- ATSC A/52:2018, section 3 (the LFE channel "is intended to be reproduced at a level +10 dB
  with respect to the fbw channels"): <https://www.atsc.org/wp-content/uploads/2021/04/A52-2018.pdf>;
  also <https://en.wikipedia.org/wiki/Bass_management>.
- SVS, "Tips for Setting the Crossover Frequency of a Subwoofer" ("The most common crossover
  frequency recommended (and the THX standard) is 80 Hz"):
  <https://www.svsound.com/blogs/subwoofer-setup-and-tuning/tips-for-setting-the-proper-crossover-frequency-for-a-subwoofer>.
- J. T. Geiger, P. Grosche, Y. Lacouture Parodi, "Dialogue enhancement of stereo sound", EUSIPCO
  2015 (the "simple center extraction and gain" baseline, "amplified (by 3.8 dB)"):
  <https://www.eurasip.org/Proceedings/Eusipco/Eusipco2015/papers/1570096395.pdf>.

- [BS] ITU-R BS.775-4 (12/2022), "Multichannel stereophonic sound system with and without
  accompanying picture", Annex 4 Table 2 (the downmix equations) and Annex 7 (the LFE):
  <https://www.itu.int/dms_pubrec/itu-r/rec/bs/R-REC-BS.775-4-202212-I!!PDF-E.pdf>.
- [DS] R. Dressler, "Dolby Surround Pro Logic Decoder Principles of Operation", sections 1.1,
  1.2 and 2 (the matrix encoder, the passive decoder, the surround's delay and 7 kHz filter):
  <https://educypedia.org/library/208_Dolby_Surround_Pro_Logic_Decoder.pdf>.
- [SOS] Sound On Sound, "Surround Sound Explained: Part 2" (the 100 Hz..7 kHz band, "about
  20mS"): <https://www.soundonsound.com/techniques/surround-sound-explained-part-2>.
- [MS] Microsoft, `KSAUDIO_CHANNEL_CONFIG` (`KSAUDIO_SPEAKER_5POINT1` with back speakers,
  `KSAUDIO_SPEAKER_5POINT1_SURROUND` with side speakers):
  <https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/ksmedia/ns-ksmedia-ksaudio_channel_config>.

Read 2026-10-05, for the announcement mixer (documentation pages only; neither project's source
was opened):

- [EM] ESPHome, "Mixer Speaker", the `mixer_speaker.apply_ducking` action (`decibel_reduction`:
  "The reduction of the media stream in decibels. Must be between 0 and 50."; `duration`: "The
  length of time to transition between the current reduction level and the new reduction
  level."; the example `decibel_reduction: 20`, `duration: 2.0s`):
  <https://esphome.io/components/speaker/mixer/>.
- [AD] Audacity Manual, "Auto Duck" (duck amount -12 dB; fade down and fade up lengths, each
  "default: 0.5 seconds"): <https://manual.audacityteam.org/man/auto_duck.html>.
- [AF] Android Developers, "Manage audio focus" (ducking is "temporarily reducing the audio level
  of one app so that another can be heard clearly"; "The start of the notification playback is
  synchronized with the end of the ducking ramp"):
  <https://developer.android.com/media/optimize/audio-focus>.

No GPL source was opened. The code is written here from the formulas above.

## Regenerating the fixtures

`python3 tools/dsp-fixtures/generate.py` (by hand, never by the gate) rewrites `fixtures/dsp/`:
every expected value from the cited numbers and formulas, computed in Python independently of
both implementations, then the chain fixtures' `samples.<o>` lines from the Rust chain
(`cargo run -p chorus-dsp --example chain_samples -- <fixture>`).
