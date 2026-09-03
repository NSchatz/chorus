# 0013: the measurement rig, and the error it publishes with its numbers

- Status: decided
- BRIEF.md section 10, first bullet; roadmap phase `chorus#RIG-3`
- Recorded by: RIG-3 (spec S0026-chorus-rig-3)
- Evidence commit: `150e3dc689d8ba544d2a268e0316392f98174a47`, the tree this
  phase started from. At that commit there is no `tools/measure/`, no
  `crates/measure/`, no `fixtures/measure/` and no sync-measurement report in
  `docs/measurements/`; the directory holds `README.md` and
  `hostctl-thread-inventory-repeat.md`, which is a flake-repeat measurement for
  the thread inventory and not a capture of anything. The check is
  `git ls-tree -r --name-only 150e3dc -- tools docs/measurements fixtures`.

## What this decides

Not that a measurement rig exists - the roadmap decided that, and BRIEF.md
guardrail 3 decided why. This entry records the choices inside it that a later
phase will want to argue with, and the accuracy the rig is entitled to claim.

Every number below is a value this rig DECLARES about itself and compares its
own work against. None of them is a target for the system being measured. The
phase is explicit that "the baseline is what the rig MEASURES, not a borrowed
ppm range it checks itself against", and nothing in `crates/measure` compares a
measured figure against a threshold of any kind.

## The estimator

**Sliding-window cross-correlation, peak located on a continuum.**

One analysis window per chirp sweep (20 ms, 1920 frames at 96 kHz), stepped an
eighth of a sweep at a time (2.5 ms), correlated against the other channel over
a declared search range of +/-1000 us. Each window yields one lag; the windows
are reduced to a signed median and to the 95th percentile and maximum of the
absolute lag.

### Why the peak is reconstructed rather than fitted

One sample at 96 kHz is 10.4 us. The phase asks for "a resolution of 10 us or
better", so a whole-sample estimator is already outside the budget before it
makes a single error. Something sub-sample is not optional.

The obvious sub-sample method is a parabola through the three integer
correlation samples around the peak, and it is the one this entry rejects. A
parabola is not the shape of a correlation peak, and the error it makes is not
noise: it is a bias set by the shape of the chirp and the fractional part of the
true delay. That is the failure mode this whole rig exists to avoid, because it
produces a confident number that is wrong in a way no amount of averaging
removes.

What is used instead: the signals are band limited to the chirp's band, so their
cross-correlation is band limited to the same band, and the discrete correlation
sequence is a SAMPLED version of a continuous function. A windowed-sinc
(Whittaker-Shannon) reconstruction with 16 taps either side and a Hann window
recovers that function; it is evaluated on a grid of 1/256 of a sample across
the sample either side of the integer peak, and a parabola through the best
three FINE points gives the vertex. The parabola is now fitted across 1/128 of a
sample rather than across two whole ones, where the peak really is locally
quadratic.

The raw correlation is what is reconstructed, not the normalised one. The
normalised coefficient's divisor moves with the lag, so it is not a sampled
band-limited function of the lag and reconstructing it would be reconstructing
the wrong thing. The normalised coefficient is used for the confidence floor and
for nothing else.

### The accuracy this buys, measured

Against `fixtures/measure/01-chirp-pair-a.wav`, whose parameter file declares a
delay of 254.0 us, which is 24.384 samples at 96 kHz:

| figure | value | error against the declared 254.0 us |
|---|---|---|
| median | +253.981 us | 0.019 us |
| p95 (absolute) | 254.026 us | 0.026 us |
| maximum (absolute) | 254.039 us | 0.039 us |

32 windows offered, 32 used, weakest correlation 0.9877, spread +253.902 us to
+254.039 us.

So the budget is 10 us and the demonstrated error against a known delay is under
0.05 us, a factor of 250 of headroom. That headroom is the point: the fixtures
are noiseless-ish synthetic captures and a real capture through two amplifiers,
two loudspeakers and a room will be very much worse. What this establishes is
that the ESTIMATOR is not the thing that spends the budget. What a real capture
spends it on is an open question and an operator-graded one.

### The tolerances the suite asserts, and where they come from

- **10 us on every reported lag figure**, against a fixture whose delay is
  declared and is deliberately not a whole number of samples. That is the
  phase's own number.
- **Two fixtures 10 us apart report different medians.** Accuracy and
  quantisation are two questions. A coarse estimator that happened to land close
  satisfies the first alone; a fine-grained wrong one satisfies the second
  alone. `crates/measure/tests/lag_resolution.rs` asserts both, and asserts the
  difference is the right SIZE rather than merely non-zero.
- **0.5 ppm on a noiseless free-run series and 5 ppm on the jittered one.**
  These are this repository's numbers and not the phase's, which names no ppm
  accuracy. They are chosen against BRIEF.md section 6: 20 to 50 ppm per device
  and about 100 ppm relative worst case. An estimator that could not separate
  two devices 20 ppm apart would be useless for the question, and 5 ppm under
  realistic jitter separates them with room to spare. The noiseless bound is
  tighter because with no noise an exact answer is available, and an estimator
  that cannot be exact where exactness is on offer has a leak in it - the same
  argument `fixtures/sync/05-noiseless-control.cfg` makes for the simulator.

## The declared floors, and what happens under them

All in `config/measure.conf`, so a run and the thresholds it was graded against
cannot drift apart, and so a reader of a report can look up every number in it.

| floor | value | why there |
|---|---|---|
| confidence floor | 0.6 normalised correlation | two independent noise sources over one 1920-sample window reach about 0.08 as the maximum over 193 lags. 0.6 is far above chance and far below what a shared chirp produces, which is above 0.98 on the committed fixtures |
| silence floor | -80 dBFS | below any real line output, above a bit-exact zero |
| chirp band fraction | 0.5 of channel energy in 1 kHz to 8 kHz | full-band noise scores 0.13 there and a real sweep 0.99, so the two are separated by a factor of seven and the floor sits between them |
| search range | +/-1000 us | wide enough for anything this project would call synchronised, and narrow enough that the periodic chirp's repeat at 20 ms cannot be mistaken for the peak |
| windows required | 4 | a median of one window is not a distribution |
| free-run minimum | 30 observations over 60 s | 30 is what makes the normal approximation to Student's t honest; the published half-width is about 4% optimistic at exactly 30 and closer than 1% at the hundreds a real run carries |
| free-run half-width | 1.0 ppm | a slope this rig cannot bound to within 1 ppm cannot tell a 20 ppm device from a 40 ppm one, so it is not published |

A run that hits any of these refuses, names which condition it hit, and writes
no report. `documentation/roadmaps/chorus.md` states the fail-safe as one
sentence and this is it: "a run that cannot resolve the two outputs says so and
writes no number."

Four conditions end a lag run, not three. The phase's own three are a silent
channel, no chirp present, and a correlation peak below the confidence floor.
The fourth is a confident peak sitting on the EDGE of the declared search range,
where the true peak may be outside it: reporting 1000 us there would be a
confident wrong number, which is the class of failure everything above is
arranged against. It is named separately so an operator whose endpoints are
further apart than the range knows to widen the range rather than to distrust
the rig.

## The amplitude ceiling

`chirp_amplitude_ceiling = 0.25` full scale, enforced in `ChirpSpec::new`, which
is the only constructor of the thing the device-backed entry point plays.

This is added by this phase and is not in the roadmap. The phase's fail-safe
covers a run that cannot resolve its outputs; it says nothing about the chirp
itself, and the chirp is the one part of this rig that drives a physical
loudspeaker through an amplifier. A sustained broadband sweep at full scale
damages a driver rather than failing a test. Everything else this phase adds is
a file that a revert undoes; that is not.

Two properties the implementation holds, both asserted:

1. **The check is in the constructor, not beside the device.** There is no path
   from a requested amplitude to a device that does not pass the refusal first,
   so "emits no audio" is structural rather than a matter of ordering that a
   later edit could get wrong.
2. **The amplitude is checked before the device is probed.** An operator asking
   for an unsafe level on a machine with no capture device is told about the
   LEVEL, because the request is what is dangerous and it will be just as
   dangerous on the machine that does have the device.

The comparison is written `if !(amplitude <= ceiling)` rather than
`if amplitude > ceiling`, because every comparison against NaN is false and the
second spelling lets `--amplitude NaN` through. There is a test for that.

## Zero dependencies, kept

The WAV reader, the cross-correlation, the sinc reconstruction, the discrete
Fourier transform behind the chirp-presence check, the least-squares fit and the
seeded PRNG are all written here. `docs/decisions/0002` records that this
repository carries zero third-party dependencies and no lockfile, and that
landing the first one is a decision-log entry rather than a quiet change. None
was needed: each of these is small and instructive, which is the case BRIEF.md
3.2 says to build rather than vendor.

The one duplication worth naming: the SplitMix64 generator in
`crates/measure/src/rng.rs` also exists in `crates/sync/src/rng.rs`. Sharing it
would have put a dependency edge from the measurement rig onto the servo
simulator's crate, and this phase is ordered before the servo precisely so that
the harness cannot be written to agree with it. Twenty lines is a cheaper price
than that edge.

## The fixtures

At the repository root in `fixtures/measure/`, beside `fixtures/protocol/` and
`fixtures/sync/`, because `docs/decisions/0002` puts them there: a
second-language implementation has to read them without a Cargo project.

Both the inputs and the parameters that made them are committed. The inputs,
because guardrail 3 wants a claim someone else can check and nobody can check a
number computed from a capture they do not have. The parameters, because a file
in the tree with no recipe is a magic number. `make measure-fixtures`
regenerates every input and the suite asserts the result is byte-identical to
what is committed.

**The caveat on "byte-identical", stated rather than discovered.** The waveforms
go through `sin`, `cos` and `ln` in the platform's maths library, which is not
required to be correctly rounded and may differ by one unit in the last place
between implementations. Every sample is then rounded to 16 bits, and one ulp
near full scale is about 2e-12 of a quantisation step, so a differing last bit
would have to land within 2e-12 of a rounding boundary to change a byte. If that
ever happens the regeneration test goes red and says which file, which is the
behaviour wanted; the alternative is a fixture that drifts quietly.

The degenerate set is what makes the refusals gradeable, and one of them is
worth explaining because it looks redundant and is not.
`06-unresolvable-chirps.wav` carries the chirp on one channel and the same chirp
reversed in time on the other. Reversing a real signal leaves its spectrum's
magnitude untouched, so both channels are loud and both pass the chirp-presence
check; what fails is the correlation, at 0.068 against a floor of 0.6. Without
it, "the correlation peak falls below the confidence floor" would have no
committed input of its own and would be a behaviour nobody had ever run.

## Time

Nothing derives a published figure from a clock read during analysis. Lags come
from sample indices and a declared sample rate. Drift comes from timestamps the
series file carries, which `docs/protocol.md` already requires to be nanoseconds
from a monotonic source. Where elapsed time IS taken - how long a recording ran,
how long an analysis took, both diagnostics - it comes from
`chorus_audio::MonotonicTimeline`, this repository's one monotonic time base.

The single settable-clock read in the crate is the human-readable date in a
report header. `audio-path.conf` records that exclusion with its reason, the way
it already does for the delay-log writer, and
`crates/measure/tests/no_settable_wall_clock.rs` asserts that it is the ONLY
unit of the crate that reads one - which an exclusion cannot assert about
itself, because an excluded unit is not scanned.

## What this rig does not do

- It does not correct, discipline or tune anything. That is SYNC-4, and it is
  measured BY this rig rather than built with it.
- It holds no view about whether a measured number is good. There is no target
  anywhere in `crates/measure`, and the free-run baseline is recorded as a
  measurement with its provenance, never as a threshold.
- It has never been pointed at hardware. Every figure in this entry is from a
  committed synthetic capture. `docs/verification-record.md` says which criteria
  of this phase are operator graded and why.

## Revisit when

The first real capture is taken, which will say what the analog path spends of
the budget this estimator does not; or a capture rate above 100 kHz makes one
sample finer than 10 us, at which point `fixtures/measure/` stops demonstrating
anything about sub-sample resolution and needs a tighter delay; or the servo
exists and a run wants to compare a corrected number against the free-run
baseline, which is the first time the baseline will be read for anything other
than a citation.
