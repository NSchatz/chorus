# 0006: the deterministic sync simulator and the servo it exercises

- Status: decided
- Recorded by: FOUNDATION-1 (spec S0001-chorus-foundation-1)
- BRIEF.md 5.3 asks for exactly this before any hardware exists

## Decision

`crates/sync` is a pure library with no clock read, no socket and no audio
device. It models two virtual clocks, a network with injected jitter, a
time-sync exchange, a filter, and a two-tier servo, and it emits a modelled
playout-error series. Everything it does is a pure function of a committed
configuration, so the same configuration produces the same series on every run
and on every machine.

## The model

Real time is the independent variable and is exact. Everything else is measured
against it.

- **Server clock**: `server_ns(t) = t * (1 + server_ppm * 1e-6)`.
- **Client clock**: `client_ns(t) = t * (1 + client_ppm * 1e-6) + initial_offset_ns`.
  Two crystals, each with its own error, and an arbitrary offset between their
  epochs because both are monotonic sources with no shared origin (BRIEF.md
  guardrail 4: no wall clock anywhere in this path).
- **Playout pointer**: where the client believes the server timeline is, which
  is what a DAC would actually be fed from. It advances at the client's crystal
  rate plus whatever correction the servo has applied:
  `playout += dt * (1 + (client_ppm + correction_ppm) * 1e-6)`.
  It starts at the client's own clock, so a run begins wrong by
  `initial_offset_ns` and has to be driven in.
- **Modelled playout error** is `playout - server_ns(t)`, sampled every step.
  This is ground truth that no participant can observe; it is the series the
  regression asserts on.

Every `sync_interval_ms` the client runs one RFC 5905 section 8 exchange:

```
t0 = client_ns(t)                                        client transmit
t1 = server_ns(t + d_forward)                            server receive
t2 = server_ns(t + d_forward + turnaround)               server transmit
t3 = client_ns(t + d_forward + turnaround + d_return)    client receive

offset = ((t1 - t0) + (t2 - t3)) / 2
rtt    = (t3 - t0) - (t2 - t1)
```

`d_forward` and `d_return` are each the configured base one-way delay plus an
independent jitter sample. Queuing only ever adds delay, so jitter is
non-negative, which is what makes the minimum-RTT filter below work at all.

## Filter and servo

Straight from BRIEF.md 5.3, which lists the proven toolkit: prefer the
minimum-RTT sample in a sliding window, then smooth lightly.

- **Minimum-RTT window**, 8 samples. The least-queued exchange is the one whose
  forward and return delays are closest to symmetric, so its offset estimate is
  the most trustworthy in the window.
- **Projection to now**, which BRIEF.md does not mention and the simulator
  found the hard way. It has its own section below.
- **Exponential smoothing**, alpha 0.25, on the projected offset, with the
  previous estimate carried forward at the drift rate before it is blended.
- **Two tiers**, as BRIEF.md 5.3 describes:
  - **Hard resync** when the observed error is at least 3 ms: step the playout
    pointer to the estimated server timeline and reset the servo. This is the
    "mute, realign, resume" tier and it is what acquires the timeline at the
    start of a run.
  - **Fine correction** otherwise: a proportional-plus-integral law on the
    filtered error, output clamped to +/- 500 ppm.

Gains are `kp = 0.4`, `ki = 0.08`, expressed against a normalised error (the
ppm that would produce the observed error over one sync interval), so the loop
behaves the same whether exchanges are a second or a tenth of a second apart.
The closed loop for that pair has poles of magnitude `sqrt(1 - kp) = 0.775`,
so it is stable with a mild, quickly damped oscillation and no ringing. The
integral term is what learns the constant relative skew; the anti-windup rule
is that the integral does not accumulate while the output is clamped.

The +/- 500 ppm clamp is BRIEF.md 5.3's reference constant, and it bounds what
the servo can correct: a relative skew above 500 ppm cannot be tracked. Real
crystals are +/- 20 to 50 ppm each, so the committed scenarios stay inside
+/- 100 ppm relative and the clamp is only exercised during acquisition.

## A stale offset is a wrong offset, and it dominated everything else

This is the one thing in this phase that measurement changed rather than
confirmed, so it is written down at length.

The first working version filtered exactly as BRIEF.md 5.3 describes: keep a
window, pick the minimum-RTT sample, smooth it. The regression passed, and the
numbers it printed were much worse than the model predicted:

| scenario | relative skew | peak modelled error |
|---|---|---|
| wired-quiet | 40 ppm | 381 us |
| wired-loaded | 50.5 ppm | 438 us |
| worst-case-skew | 100 ppm | 872 us |

Against a 1 ms bound, the worst case had 13% of margin left, and jitter was
not what was eating it: the quiet scenario injects at most 60 us of queuing,
and its error was six times that.

The cause is that a sliding window hands back a measurement that was taken up
to a window ago, and the offset between two crystals is moving the whole time.
The minimum-RTT rule makes this worse rather than better, because it
deliberately prefers an old clean sample over a fresh noisy one. The error
term is `relative skew * age of the selected sample`: at 100 ppm and an
8 second window that is 800 us, which is essentially the whole 872 us that was
measured. The exponential smoother adds its own lag on top, `(1 - alpha) /
alpha` intervals, which is another 3 seconds at alpha 0.25.

So the filter now projects the sample it selects forward to the present, at a
drift rate it estimates from the offsets themselves, and carries the previous
smoothed estimate forward the same way before blending. With that, the
remaining error is the drift estimate's own error times the sample age, which
is small and does not scale with the skew:

| scenario | relative skew | before | after | steady state after |
|---|---|---|---|---|
| wired-quiet | 40 ppm | 381 us | 116 us | 47 us |
| wired-loaded | 50.5 ppm | 438 us | 159 us | 144 us |
| worst-case-skew | 100 ppm | 872 us | 343 us | 110 us |

"After" is the peak from the moment the run comes inside the bound, which
still includes the tail of the acquisition transient. "Steady state" is the
peak over the back half of the run. Both are printed by the regression on
every CI run, so the next change to this code is measured against these
numbers rather than against a passing assertion.

**The drift rate is deliberately not taken from the servo's correction**, even
though the servo has exactly that quantity and knows it far more precisely.
Doing so would close a loop: the correction changes the projected offset,
which changes the observed error, which changes the correction. The gain
around that loop is `kp * age / interval`, which at these gains and this
window is about 1.6, and positive. It would not oscillate, it would run away.
An estimate taken from the measurements is noisier and open loop, and open
loop is the property that matters here.

The estimator is the slope between the least-queued sample of the older half
of the window and the least-queued sample of the newer half, which needs no
history beyond the window that already exists. It is clamped to 2500 ppm,
slightly more than two crystals at the extreme of the modelled range, so one
unlucky pair cannot throw it.

What this is worth outside the simulator: this is a real defect that a real
implementation would have, and it is exactly the class of thing that would be
diagnosed as "the servo is badly tuned" on hardware, where the true error is
not observable. It cost nothing to find here.

## What this deliberately does not model

Honesty about the model matters more than fidelity here, because everything
downstream is measured on hardware anyway (BRIEF.md guardrail 3: "sounds
synced" is not evidence, and neither is "simulates synced").

- The exchange is instantaneous in model time: its round trip affects the
  timestamps but not when the servo acts. At a 1 s cadence against a sub-ms
  round trip that is noise.
- Single-sample insertion and deletion is modelled as a continuous rate
  correction rather than as discrete sample events. The granularity of a real
  correction at 48 kHz is 20.8 us of playout per inserted sample, which is
  below the resolution this model is claiming.
- No DAC delay accounting, no buffer model, no packet loss, no reordering.
  Those need the measurement rig (RIG-3) to be worth anything.
- BRIEF.md 5.3's "burst on connect" is not modelled: the hard-resync tier
  acquires the timeline on the first exchange, so a burst would only change how
  fast a number this model cannot yet validate gets there.

The simulator's job is to keep servo logic honest in CI, not to predict a
number. The number comes from the rig.

## Determinism

- The PRNG is a SplitMix64 written into this repository (`src/rng.rs`), seeded
  by the scenario. It is integer arithmetic with no platform-dependent
  behaviour, and it is small enough to mirror exactly in C when the firmware
  wants the same stream.
- Jitter samples are drawn in a fixed order, one forward then one return, per
  exchange.
- The error series is recorded as integer nanoseconds, so a run is compared to
  another run exactly rather than within a tolerance.
- No time is read, no thread is spawned, no map is iterated.

Two runs of one scenario are therefore identical, and a run is identical across
machines to the extent that IEEE 754 double arithmetic is, which for the
ordinary add and multiply used here is exact reproducibility.

## Configuration validity

The simulator refuses a configuration rather than reporting a result for one it
cannot model. Documented valid ranges:

| parameter | valid | why |
|---|---|---|
| `duration_ms` | at least 1, and at least one step | a zero-length run has no series to report |
| `step_ms` | 1 or more, not longer than the run | the sampling grain |
| `sync_interval_ms` | 1 or more | the exchange cadence |
| `server_ppm`, `client_ppm` | finite, absolute value at most 1000 | 1000 ppm is 20x the worst real crystal pair; beyond it the linear clock model stops describing a crystal |
| `initial_offset_ns` | any i64 | two monotonic epochs are unrelated by construction |
| `base_one_way_delay_us` | finite, 0 to 100000 | a 100 ms one-way delay is already off this LAN |
| jitter scale | finite, 0 to 100000 us | same bound, same reason |
| total steps | at most 10 million | a guard against a configuration that would run for hours in CI |

Anything outside these is a rejected configuration and no playout-error result
is produced for it.

Exponential jitter has an unbounded tail, which no real queue has, so samples
are capped at 10x the configured mean. The cap is part of the model and is
documented here rather than hidden in the code: a real queue is bounded by
buffer depth, and an uncapped draw would occasionally hand the filter a delay
no switch on this network could produce.

## Committed scenarios

`fixtures/sync/*.cfg`, one file per scenario, each carrying its own settle
deadline and error bound so the assertion travels with the configuration
instead of living in a test. They cover a quiet wired link, a loaded one with
exponentially distributed queuing, the worst realistic crystal pair, and an
acquisition that starts inside the hard-resync threshold so the fine tier has
to pull it in against the clamp.

## Revisit when

The measurement rig (RIG-3) produces real jitter histograms and real drift
traces. At that point the scenarios stop being plausible and start being
measured, the gains get tuned against both, and this entry gets a successor
that cites numbers instead of reference constants.
