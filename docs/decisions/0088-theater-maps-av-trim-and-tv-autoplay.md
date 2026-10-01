# 0088: the endpoint DSP chain plays a stereo TV into a theater set and a 5.1 stream into a pair or one speaker by cited maps, a room carries a signed A/V trim and a TV upmix, and a TV's standby stops its autoplay at once

- Status: accepted (goal 13, 2026-10-01)
- Decided by: the goal (program section 17, P2 Option A approved at Checkpoint K; K81) inside
  the coordinator's goal-13 design envelope, track `chorus-g13/theater`; every default below
  not cited is ASSUMED
- Implemented in: `crates/dsp/src/chain.rs`, `settings.rs`, `fixture.rs` and
  `firmware/src/dsp.c`, `firmware/include/chorus/dsp.h` (the maps), `fixtures/dsp/chain-theater-*`
  (13 shared fixtures, `tools/dsp-fixtures/generate.py`); `crates/protocol/src/v2/`
  (`Sound::tv_upmix`, `Sound::fold`, `sound_fold`, `SourceOffer::reason`, `signal_reason`),
  `firmware/src/protocol_v2.c`, `firmware/src/endpoint_dsp.c`, `fixtures/protocol/v2/`
  (`sound_theater_ambient`, `sound_stereo_fold`, `source_offer_standby` and three rejections);
  `crates/control/src/theater.rs` (new: `TvUpmix`, `AV_TRIM_MS`, `tv_play_at_lead_ns`),
  `catalog.rs` (`av_trim`, `sound`'s `tv_upmix`, autoplay's `stop_on_standby` and `low_latency`),
  `zones.rs`, `persist.rs` (state-file format 4), `fixtures/control/v2/` (4 command vectors, 4
  refusals, `state-rich` extended); `crates/server/src/control.rs` (`sound_of`, `fold_of`),
  `linein.rs` (`InputEvent::Standby`), `conductor.rs`, `schedule_runtime.rs`
  (`on_input_standby`); `crates/client-linux/src/dsp.rs` (`settings_from`). The contract is
  `docs/dsp.md` ("The theater maps"), `docs/protocol.md` (0x39, 0x36, "The channel map") and
  `docs/control-plane.md` ("The TV path", "TV autoplay", state-file format 4)

## Context

Goal 13 builds the TV path (stereo LPCM from a TV's optical or ARC output, Option A). This track
gives it the model and the chain: what each member of a theater set plays when the TV is
stereo, what a stereo pair or a single speaker plays when the stream is 5.1, the room's A/V
trim the relay applies to its stamps, the room's choice of a surround upmix, and the K81 rule
that a TV's standby ends its autoplay without the line-in hold. Theater bonding end to end, the
relay and CEC are other tracks'.

## What was read

All read 2026-10-01: `BRIEF.md` section 3.1; the program's sections 0.3, 0.4, 0.8, 0.9 and 4;
the goal-13 design envelope and research survey; ADRs 0076, 0079, 0081, 0082 and 0085;
`docs/dsp.md`, `docs/protocol.md`, `docs/control-plane.md`; the code each "Implemented in"
names. Sources: ITU-R BS.775-4 (12/2022), Annex 4 Table 2 and Annex 7 (the PDF, text
extracted); R. Dressler, "Dolby Surround Pro Logic Decoder Principles of Operation" (PDF);
Sound On Sound, "Surround Sound Explained: Part 2"; Microsoft Learn, `KSAUDIO_CHANNEL_CONFIG`
and `KSPROPERTY_AUDIO_CHANNEL_CONFIG`; for the CTA-861 order, the NXP i.MX 6 and Rockchip
RK312x HDMI transmitter chapters (they name the channel-allocation register, not its table)
and search summaries. URLs are in `docs/dsp.md` "Citations" and `docs/protocol.md`. No GPL
source was opened: the search for CTA-861's table turned up Linux driver source (GPL), which
was not opened.

## Decision

1. **The maps live in the shared chain, step 6**, as one more `Source`: a weighted sum of up
   to seven stream channels in a fixed term order (`Mix`), two of them for a stereo downmix
   (`Downmix`), and the ambient surround (`Ambient`). Every coefficient is 1/sqrt 2 (BS.775-4
   prints 0.7071) or a half of it; each is designed in f64 and rounded to f32 once, so the C
   and Rust chains agree sample for sample on the 13 new fixtures (the existing 39 fixtures and
   their golden samples are unchanged byte for byte: the generator was re-run and diffed).
2. **A stereo stream into a theater set**: the centre member plays `(FL + FR) / sqrt 2`, the
   passive matrix decoder's centre (Dressler 1.1: the encoder divides the centre equally into
   Lt and Rt at -3 dB; the inverse is computed); the front members play FL and FR whole; the
   surround members play silence (`tv_upmix` off, the default) or `(FL - FR) / sqrt 2`
   band-limited to 100 Hz..7 kHz and 20 ms late (`ambient`: Dressler 1.2, Sound On Sound); the
   sub is goal 12's bass management. The 20 ms rides the output's delay line, so it is part of
   `output_delay_us` and not of the reported latency (an endpoint's sync must not take it back).
3. **A 5.1 stream into a stereo pair or one speaker**: BS.775-4 Table 2. A front member of a
   set with no centre folds `FC / sqrt 2`, with no surrounds its side's surround(s) at
   `1 / sqrt 2` (the 2/0, 3/0 and 2/2 rows); one unbonded stereo speaker with `stereo_downmix`
   takes the 2/0 pair as two outputs; one unbonded two-way speaker takes the mean of that pair
   (the chain already takes the mean of a stereo pair for a two-way, and Table 2's own 1/0 row
   would play a 5.1 mix 3 dB above the same programme in stereo). The LFE is dropped from a
   downmix ("often not included in a 2-channel downmix", Annex 7).
4. **A surround role takes 5.1's other naming**: SL plays BL when the stream has no SL, and so
   on (Microsoft's two 5.1 configurations), so the server's 6-channel map (BL BR) plays on a set
   whose surrounds are side members.
5. **The wire**: `sound` (0x39) gains an optional two-byte theater block after the filters,
   `tv_upmix` and `fold`, written only when either is not 0, so every goal-12 vector keeps its
   bytes, and an older decoder ignores it as excess (the v2 extension rule). The server sets
   `fold` from the room's bond, only for an FL or FR member. `source_offer` (0x36) gains an
   optional `reason` after `name` (0 none, 1 standby, 2 non_pcm), on the same rule. Rust and C,
   golden and rejection vectors both directions.
6. **The catalog**: `av_trim` (v2) sets `av_trim_ms`, -100..200 ms, positive delays the audio;
   `sound` takes `tv_upmix` (`off`, `ambient`); an autoplay rule takes `stop_on_standby` and
   `low_latency`, both default true and written only when false (so goal 11's autoplay bytes
   hold in commands and the state). The state carries `av_trim_ms` per room and `tv_upmix` in
   its `sound`. State-file format 4 adds the four fields, each required in a format 4 file; a
   format 1, 2 or 3 file loads with the defaults and the next write is format 4.
7. **The relay's rule is a pure helper**: `tv_play_at_lead_ns(l_tv_ns, l_floor_ns, trim_ms)`
   returns `max(l_floor, l_tv + trim)` and whether the floor clamped it; the caller logs
   `av-trim-clamped`. Saturating, never wrapping; tested on every trim the catalog accepts.
8. **TV autoplay (K81)**: TV on is an ordinary signal, and the rule's `take` already takes the
   target out of its group (tested). A `source_offer` with `signal` false and `reason` standby
   from an `optical` or `hdmi_arc` input is a new `InputEvent::Standby` (said even when the
   signal was already off); the runtime's `on_input_standby` ends an autoplay of it whose rule
   says `stop_on_standby` at once and restores its rooms; otherwise, and for any other input
   kind or reason, it is a signal gone with its 30 s hold.

## Deviations from the envelope

- The envelope names the HDMI/CTA-861 5.1 order to be "confirmed or kept as a LEAD": kept as a
  LEAD, with why (the standard is sold, the manuals found do not print the table, the drivers
  that do are GPL). Nothing here depends on it.
- The envelope's chain table did not say how a set's layout reaches the endpoint: the `fold`
  bits are new, in the `sound` block, set by the server from the bond.
- The envelope's single-endpoint downmix needs the endpoint to know it is one stereo speaker:
  `EndpointDsp::stereo_downmix` (C `chorus_dsp_endpoint_t.stereo_downmix`) is new, and no
  endpoint sets it yet (a follow-up).
- `source_offer`'s `reason` carries `non_pcm` too, for the capture track's refusal; the
  server treats it as a signal gone.
- The goal-12 test `sound_on_the_wire` asserted that the members of a 2.1 set differ only in
  their role; it now says "and the front member's fold". `sound_v2`'s two assertions on the
  state shape and the file format now expect `tv_upmix` and format 4. Nothing else changed.

## ASSUMED values

`tv_upmix` off by default; the A/V trim bounds -100..+200 ms; the ambient band's slopes (2nd
order, Q 1/sqrt 2; the corners are cited); folding both side and back surrounds of a 7.1 stream
at 1/sqrt 2 each (BS.775 is a 5-channel recommendation); `stop_on_standby` and `low_latency`
true by default. The TV-dependent ones wait on the Needs item "The three TVs: model, eARC port,
optical out and audio menu".

## Not chosen

- **An active (steering) upmix** such as Pro Logic II: its decoder is not a published formula
  this project can cite and test against, and the passive matrix is.
- **Subtracting the centre from the fronts** when a centre plays `(FL + FR) / sqrt 2`: the
  envelope keeps FL and FR whole, and a subtraction would narrow the stage for every stereo
  source.
- **Server-side downmixing**: every member gets the room's whole stream (ADR 0081), and the
  chain is where bass management already is.
- **Writing the theater block always**: it would change every committed `sound` vector.

## Follow-ups (the integration track)

- Call `tv_play_at_lead_ns` in the TV relay with the room's `av_trim_ms` and log
  `av-trim-clamped`; honour the rule's `low_latency`.
- The hub's CEC standby: send `source_offer` with `signal` false and `reason` 1 for its TV
  input; the capture track's non-PCM refusal: `reason` 2.
- Set `stereo_downmix` on a Linux client playing a 2-channel device without an output map, and
  on the C endpoint once it negotiates more than 2 channels (chorus#TV-9).
- Confirm the CTA-861 5.1 order before any 5.1 HDMI input (Option B).
