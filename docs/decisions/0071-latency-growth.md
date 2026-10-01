# 0071: a line-in's latency grows from L_local to L_group by a bounded, raised-cosine time stretch planned on the server, proven against a stated glitch criterion in the simulator

- Status: accepted (goal 11, 2026-10-01)
- Decided by: the goal (item 4, K94); cheap to reverse (a pure library nothing calls yet), so
  made and recorded here
- Implemented in: `crates/sync/src/latency_grow.rs` (the plan and the resampler),
  `crates/sync/src/latency_sim.rs` (the simulator and the criterion),
  `crates/sync/src/latency_report.rs` and `crates/sync/src/bin/chorus-sim-latency.rs` (the
  report), `config/sim-latency/{wired-join,wifi-join}.latency` (the scenarios); held by
  `crates/sync/tests/latency_growth.rs` and the unit tests in both modules; `audio-path.conf`
  lists the plan on the path and excludes the simulator, the report and the bin; report
  `docs/measurements/latency-growth-sim.md`

## Context

K94: "Automatic: low latency while only the local room plays an input; the buffer grows
seamlessly when other rooms join; no audible glitch is an acceptance criterion." ADR 0066 made a
Linux endpoint's line-in a source: 20 ms PCM chunks stamped at their capture instants on the
server timeline. A line-in heard only in its own room can be played a few tens of milliseconds
after capture. A group of rooms is held to a deeper buffer: `playout_latency_us = 180000`
(`config/sync.conf`) for a wired group, `wireless_playout_latency_us = 500000`
(`config/transport.conf`) once a Wi-Fi room is in it. Moving the playing room from one to the
other at once leaves the difference with nothing to play (a gap) or, going back, plays over
itself (a drop). Goal 11's done-when line D asks for the growth with no glitch in the simulator.

## What was read

All read 2026-10-01:

- Dai, H. and Micheyl, C. (2011), "Psychometric functions for pure-tone frequency
  discrimination", J. Acoust. Soc. Am. 130(1), open access at
  <https://pmc.ncbi.nlm.nih.gov/articles/PMC3155586/>: "The smallest mean threshold, expressed as
  delta-f/f in percent, was observed for the 1000-Hz standard frequency; it was equal to 0.2%"
  (trained listeners, 35-85 dB SPL, adaptive two-interval procedure).
- The record of Wier, Jesteadt and Green (1977), "Frequency discrimination as a function of
  frequency and sensation level", J. Acoust. Soc. Am. 61(1), 178-184, DOI 10.1121/1.381251
  (its abstract only, <https://experts.nebraska.edu/en/publications/frequency-discrimination-as-a-function-of-frequency-and-sensation>):
  the classic source that discrimination is best at mid frequencies and moderate levels; not
  relied on for a number.
- chorus's own: `crates/sync/src/{sim,house,jitter,servo,config,scenario}.rs`,
  `crates/sync/tests/`, `config/sync.conf`, `config/transport.conf`, `config/sim-house/`,
  `docs/measurements/sim-house-8-rooms.md` and ADR 0048, ADR 0066, `docs/protocol.md` (0x02,
  0x11, 0x14, 0x37), BRIEF.md section 2.2, `tools/conventions/check-{shared-fixtures,measurements,provenance,adrs}.sh`.

No third-party source was opened; the resampler and the profile are written from their
definitions (Catmull-Rom is the cubic Hermite spline with central-difference tangents).

## Decision

1. **The server time-stretches; the endpoints are untouched.** One `LatencyPlan` and one
   `CubicResampler` per source stream, in the line-in's server slot, between the PCM received
   upstream and the chunks sent to the group. Output chunks stay contiguous on the server
   timeline and every room receives the same chunks with the same stamps, so the rooms stay in
   step with each other through a transition, and the joining room is in sync from its first
   chunk. What moves is which source position each output frame plays: source position
   `s_j = j - D(j)` (minus a two-frame lookahead shift, item 5), and the stamp offset is
   `L_0 + D(j) / fs` exactly.
2. **The rate profile is a raised-cosine ramp, a hold, and the mirror ramp.** The rate deviation
   `r` (positive plays slower, the offset grows) goes 0 to the peak over R as
   `p (1 - cos(pi u / R)) / 2`, holds, and comes back the same way. It never steps, its slope is
   at most `p pi / (2 R)` per frame, the growth is `p (R + H)` frames, and every position is a
   closed form (no accumulation: a ten-minute transition lands on its target). A change under
   `r_max R` frames uses a reduced peak and no hold.
3. **The bound: r_max = 500 ppm (0.05 %, 0.87 cents).** The cited threshold is the smallest mean
   frequency difference limen in Dai and Micheyl (2011), 0.2 % (3.46 cents) at 1 kHz for trained
   listeners; 500 ppm is a quarter of it, and with the endpoint servo's own largest correction
   (`max_correction_ppm = 300`) the worst case is 800 ppm (1.38 cents), still under half. That
   threshold is for two steady tones compared side by side, a harder test than a slow glide in
   music, so the margin is conservative; it is a choice, ASSUMED until a listening test. Ramp R =
   10 s, ASSUMED (the rate then changes by at most 1.57 ppm per 20 ms chunk). Both are constants,
   `MAX_RATE_DEVIATION` and `RAMP_MS`, which the scenarios are asserted to equal.
4. **What it costs in time.** A change of dL takes `dL / r_max + R`: with L_local = 30 ms
   (ASSUMED: a 20 ms chunk to fill, the wired path, 1 ms of server processing and a 4 ms endpoint
   guard, all ASSUMED) to the wired group's 180 ms, 310 s; to the wireless tier's 500 ms, 950 s.
   That is long and deliberately so: nothing waits for it except the margin. A wired room that
   joins starts at once (its chunks arrive in time at L_local). A Wi-Fi room starts once the
   offset reaches its tier's floor, read here as `wireless_min_us = 120000` (ASSUMED reading):
   185 s after the join in the simulation. That wait is the one real cost and is a follow-up for
   the owner (below).
5. **The interpolator is four-point Catmull-Rom, and its lookahead costs two frames, not a
   chunk.** It passes through every sample and reproduces a straight line exactly, which the
   simulator uses: the source signal is the frame index, so every rendered sample is the
   position played and the criterion is checked on the audio itself. It reads two frames past a
   position; if output chunks ended where source chunks end, the last two frames of every chunk
   would wait 20 ms for the next source chunk (the first run of the simulator showed exactly
   that as underruns at L_local). So the stream's first output chunk is two frames short
   (`LOOKAHEAD_FRAMES`); every later chunk then reads only its own source chunk for any offset at
   or above the initial one. Plain `f64`: this is the server, not the endpoint's hot path.
6. **Shrinking is the same mechanism with the rate reversed**, cheap, so it is in: when the
   other rooms leave the server asks for L_local and the plan plays slightly faster back down
   (310 s in the wired scenario, criterion held). A target set while a transition runs waits
   until it ends (the rate is then 0, so the next profile starts from rest); only the latest is
   kept.
7. **The glitch criterion**, asserted by `LatencyRun::failures` for the room already playing:
   (a) the rendered samples, read as source positions, strictly increase with every per-frame
   step inside [1 - r_max, 1 + r_max], and no frame is inserted (a stamp gap) or dropped (a stamp
   overlap); (b) every chunk reaches it at least the endpoint guard before its play-at stamp,
   under the modelled network; (c) the stamp offset never moves away from where the plan is
   heading and every transition reaches its target; (d) the rate's change per chunk is inside
   the profile's slope bound. For the pair once both play: |inter-room error| under BRIEF.md
   section 2.2's cross-room acceptable bound, 5 ms. The negative control (the latency set to
   L_group at once) runs through the same checks and fails (a) with exactly dL of inserted
   silence; a test asserts it.
8. **The simulator reuses the house.** The endpoints, their links (base delay, asymmetry, jitter
   model including the Wi-Fi burst model) and the servo come from
   `config/sim-house/8-rooms-switched.house` (ADR 0048); inter-room error is the difference of
   the two endpoints' single-client playout errors at each chunk's play-at instant. The
   scenario parameters are under `config/sim-latency/`, not `fixtures/sync/`, for ADR 0048's
   reason: the endpoint's C core models one endpoint and has no reader for them, and every file
   under `fixtures/sync/` must be read by both implementations. Every shared number (L_group,
   the Wi-Fi floor, r_max, R, the 20 ms chunk and 48 kHz) is asserted equal to its one copy.
9. **In `crates/sync`, not a new crate.** It is the sync engine's own concern (the stamp offset
   on the server timeline), it needs the house and jitter models for its simulator, and it adds
   no dependency; a crate of its own would re-export half of this one.
10. **The report** `docs/measurements/latency-growth-sim.md` is `Source: simulation`, written by
    the committed bin, and regenerated by a test that fails if any line but the two build lines
    differs. Its `Build measured` names the `main` commit the change was built on (the model is
    added by the same change, as its `Build note` says), because a squash merge leaves the branch
    commit out of `main`'s history and the provenance check wants an ancestor.

## Tests (no device, conventions rule 10)

`crates/sync/tests/latency_growth.rs` (7): the scenario list is complete; every shared number
equals its one copy; the wired join and leave pass the criterion with both transitions in the
planned time, the joiner starting at once with no underrun; the Wi-Fi join passes with the joiner
starting later at its floor and no underrun after; the naive jump fails (a) in both scenarios with
exactly dL inserted; a run reproduces bit for bit; the committed report is the generator's.
Unit tests: `latency_grow` (6: profile continuity and totals, growth to a target, a target held
during a transition, Catmull-Rom on a line, the resampler against the plan, bad configurations
refused) and `latency_sim` (3: the scenario parser).

## Not chosen

- **Jump the latency and mute across the splice**: the gap is the glitch K94 rules out.
- **Wait for silence and jump then**: a line-in may never fall silent (a turntable's surface
  noise), and the joining room would wait indefinitely; kept as an optimisation (below).
- **Stretch on the endpoints** (each slowing its own playout): every room would have to slew in
  lock step to stay in sync, and the joining room could not start until it did; the server
  stretching once keeps one set of stamps.
- **A pitch-preserving time stretch (WSOLA, phase vocoder)**: needed only for stretches large
  enough to hear as pitch; at under a cent resampling is simpler and has no transient artifacts.
- **A larger r_max for a faster transition**: 0.2 % is the best-case threshold itself; the
  margin is worth more than minutes.

## Follow-ups

- Integration track: wire `LatencyPlan` and `CubicResampler` into the server's line-in slot; L_local
  and L_group come from the room model (the group's tier). The joining room is sent chunks from
  the join and starts on the first one that arrives in time at its tier's floor.
- The owner: whether a Wi-Fi room's start delay (185 s from 30 ms in the simulation) is
  acceptable, or whether a line-in whose group may include Wi-Fi should start at a deeper
  L_local, or jump during detected silence.
- Capture-clock drift against the server timeline and gaps in the source (an overrun) are not
  modelled: the server would fill a gap with silence before planning, and absorb drift with the
  same plan at a tiny rate.
- A listening test of 500 ppm and the 10 s ramp on music, and a windowed-sinc interpolator if the
  cubic's artifacts are audible.
