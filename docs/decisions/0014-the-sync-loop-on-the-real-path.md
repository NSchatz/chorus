# 0014: the sync loop on the real path, and every constant it fixed

- Status: decided
- BRIEF.md section 5.3; roadmap phase `chorus#SYNC-4`
- Recorded by: SYNC-4 (spec S0031-chorus-sync-4)
- Evidence commit: `ba6ca4f783bd4a89f26360a900f045a911e0b162`, the tree this
  phase started from. At that commit `crates/sync` is a pure library with no
  caller outside its own tests, `crates/client-linux` corrects nothing and says
  so in its own module documentation, and the `time_sync` message in the
  catalog is decoded by the client into `Received::NotInThisGrammar`. The
  checks are `git diff --stat 7e50d3f ba6ca4f -- crates/sync/`, which returns
  empty, and `git grep -n 'NotInThisGrammar' ba6ca4f -- crates/client-linux`.

## What this decides

Not that there is a servo - `docs/decisions/0006-sync-simulator-and-servo.md`
decided that, against modelled clocks. This entry records what happens when
that servo is handed a real audio device and a real peer: where its error
signal comes from, what "applying a correction" means on a fixed-rate DAC, and
**every constant this phase fixed, with what each was chosen from**.

The last part is the point. The roadmap phase's scope note is explicit: "Filter
windows, thresholds and the correction clamp are OUTPUTS of this phase; no
assertion names one, and the constants relayed in the brief are another
project's starting points, not targets." So each value below carries its
provenance, and where a value happens to equal an inherited one, this entry
says so rather than letting the coincidence read as a measurement.

Every number is committed once, in `config/sync.conf`.
`crates/client-linux/tests/sync_loop.rs` asserts the file and the compiled
constants in `crates/client-linux/src/sync.rs` are the same numbers, and
`tools/sync-hour-run.sh` passes the file's values to the client on its command
line, so a check and the thing it checks cannot drift apart.

## Where the error signal comes from

**The delay the audio device reports to its final DAC, and nothing else.**

`alsa` defines it: "For playback the delay is defined as the time that a frame
that is written to the PCM stream shortly after this call will take to be
actually audible. It is as such the overall latency from the write call to the
final DAC." The loop reads it through `PcmSink::delay_frames`, which is
`snd_pcm_delay`.

The error the servo is driven with is

```
error_ns = (next_write_ts_ns + playout_latency_ns)
           - (client_now_ns + device_delay_ns + offset_ns)
```

positive when playout is ahead of where the server timeline says it should be.
`next_write_ts_ns` is the presentation timestamp of the next frame the client
will write; `client_now_ns + device_delay_ns` is when that frame becomes
audible on the client's own monotonic clock; adding the filtered offset puts
that instant on the server timeline.

**The signal that is not used, and why it is refused by name.** The time a
`write` call takes to return is available in the playout loop, is plausible,
and measures when the device ACCEPTED bytes rather than when they become
audible. A loop disciplined against it converges beautifully onto the wrong
target, so `SyncLoop` is not given access to it: `observe` takes a `PcmSink`
and calls one method on it, and `crates/client-linux/tests/sync_loop.rs`
asserts by reading the source that `SinkWrite`, `frames_written`, `underran`
and `.write(` do not occur in `crates/client-linux/src/sync.rs` outside its
documentation.

**A device that refuses to report its delay stops the run.** There is no
fallback estimate, because every available fallback is the wrong signal wearing
a different name. `StopReason::DelayRefused` names the device and says so.

**A device that answers with a delay of zero is not corrected against at all.**
That is a different case from a refusal and it gets a different answer,
`Correction::NoDeviceDelay`. Two devices report zero and both mean the same
thing. A device with no ring - the ALSA `null` device accepts every frame
instantly and reports zero for ever, which is why `tools/lib.sh` refuses it for
anything about a reported delay - has nothing for a distance to be a distance
FROM, and a loop that formed an error from it would find itself a whole playout
latency early on every tick and insert silence for as long as the run lasted. A
real device reporting zero has run dry, and `alsa` warns that on underrun the
reported delay "will not necessarily got down to 0", so a zero from a real
device is a fault reading rather than a small one. Correcting nothing is the
opposite of falling back: it is what the absence of the one admissible signal
requires.

## What "applying a correction" means on a fixed-rate DAC

A DAC plays the frames it has been given, in order, at its own rate. So writing
a chunk later shortens the ring by exactly as much as it delays the write, and
the instant a given frame becomes audible does not move at all. The only thing
that moves it is how many frames come before it.

That splits the client's job in two, and this repository keeps them apart:

- **Pacing** chooses WHEN to write, which holds the reported delay near
  `device_target_us`. It is a property of one endpoint and moves nothing.
- **Correction** changes WHAT is written. A fine correction of `c` ppm drops
  `c` frames per million when playout is ahead and inserts that many silent
  frames when it is behind. A hard resync does the whole step at once.

`PlayoutCorrector` carries the fraction rather than rounding it away: at 48 kHz
a 100 ppm correction is 4.8 frames a second, and rounding each chunk's share to
a whole frame would quantise the correction to 50 frames a second, which is
1000 ppm of granularity on a 300 ppm clamp.

**A hard resync mutes by silencing, not by inserting silence.** Inserting would
itself move the playout pointer, and the mute would then be part of the
correction instead of covering its splice.

**And the mute starts where audio resumes, not at the front of the chunk.** A
backward step inserts silence, and the join an ear would hear is at the far end
of that insertion. Silencing the front of the chunk instead spends the mute on
frames the corrector had just zeroed itself: for any backward step larger than
`mute_us` the only audible discontinuity is then uncovered, and
`muted_frames()` reports it as covered. So `PlayoutCorrector::shape` records
where the insertion ends and mutes from there, and carries the mute to the next
chunk when this one had no audio left after the splice.

## How the constants were chosen

The method, so a later reader can re-run it: a modelled endpoint
(`crates/client-linux/tests/common/mod.rs`) drives the REAL `SyncLoop` and
`PlayoutCorrector` against a modelled clock pair, a modelled network with the
jitter distributions `crates/sync/src/jitter.rs` already models, and a modelled
DAC on a virtual clock. Ground truth - how far the content about to become
audible is from where the server timeline says it should be - is computed
outside the loop, from state no participant in the model can see. Each
candidate was run for 60 modelled minutes and graded on the peak absolute
ground-truth error after the first modelled minute.

**This is a MODELLED result and is not a measurement.** It says a candidate is
not wrong in the ways the model can see. `docs/verification-record.md` records
what was and was not run, and AC-1 - two real endpoints, an hour, a rig - is
recorded there as NOT passed.

Three modelled scenarios, chosen to match the committed simulator scenarios in
`fixtures/sync/` rather than invented here:

| scenario | clocks | link |
|---|---|---|
| quiet wired | 0 and +40 ppm | 120 us base, uniform 0 to 60 us |
| wired loaded | -12.5 and +38 ppm | 200 us base, exponential 150 us mean |
| worst crystal pair | +50 and -50 ppm | 250 us base, exponential 150 us mean |

At the values fixed below, over 60 modelled minutes each, peak error after the
first modelled minute:

| scenario | ground truth | the loop's own error | hard resyncs | after minute 1 | underruns | reproduced by |
|---|---|---|---|---|---|---|
| quiet wired | 69.8 us | 42.0 us | 1 | 0 | 0 | `a_modelled_hour_holds_below_a_quarter_millisecond_after_the_first_minute` |
| wired loaded | 184.4 us | 80.3 us | 1 | 0 | 0 | `the_modelled_hour_holds_on_the_loaded_wired_link_too` |
| worst crystal pair | 158.7 us | 90.7 us | 1 | 0 | 0 | `the_modelled_hour_holds_at_the_worst_realistic_crystal_pair_too` |

Every row is a committed test in `crates/client-linux/tests/sync_loop.rs`, named
above, and the seeds and initial misalignments that make each one reproducible
are in that file rather than in this prose. Run them with

```
make one ARGS="-p chorus-client-linux --test sync_loop -- --nocapture"
```

and the figures print. The `wired loaded` row was carried as prose in the first
cut of this record, with a seed and an initial misalignment that were never
committed; the numbers here are the committed reproduction's, which is why the
row now reads 184.4 us rather than the 125.3 us that first stood here. The
scenario, its clocks and its link are unchanged.

## The constants this phase fixed

### `filter_window = 64` and `smoothing_alpha = 0.0625`

**Chosen from a sweep, over the worst-case crystal pair, 20 modelled minutes
each, peak ground-truth error after the first modelled minute:**

| window | alpha | interval | peak |
|---|---|---|---|
| 8 | 0.25 | 1000 ms | 308 us |
| 16 | 0.25 | 1000 ms | 284 us |
| 32 | 0.125 | 500 ms | 205 us |
| **64** | **0.0625** | **500 ms** | **159 us** |
| 64 | 0.03125 | 500 ms | 121 us |
| 128 | 0.03125 | 500 ms | 91 us |

The sweep is a committed test,
`the_sweep_that_fixed_the_window_the_alpha_and_the_interval_reruns` in
`crates/client-linux/tests/sync_loop.rs`. It re-runs all six rows and prints
them, and it asserts what this section rests on rather than the digits: that the
first row misses this phase's bound, that the chosen row holds it, and that the
column keeps improving to the bottom.

The first row is the shape `ServoConfig::default()` carried out of
FOUNDATION-1, and it is the one row that does not hold this phase's own bound.
That is the whole reason these are outputs of this phase rather than inherited:
a window and a smoothing weight tuned against a 1 ms bound do not survive being
held to a 0.25 ms one.

Deeper windows and lighter smoothing keep helping, and the choice stops at
64 and 1/16 rather than 128 and 1/32 for a reason that the sweep cannot show:
64 exchanges at 500 ms is 32 seconds of history, and the offset the filter
selects is projected forward across that span at a drift rate estimated from
the samples themselves. Doubling the span doubles what that projection is
asked to carry, for 30 us of a budget that already has 90 us of margin. The
margin was spent on a shorter memory rather than on a smaller number.

### `sync_interval_ms = 500`

**Chosen from the same sweep**, which is why it appears in that table. Two
exchanges a second, against the one a second the simulator's reference cadence
used. It is the cadence that made a 64-deep window a 32-second history rather
than a 64-second one; the cost is a 35-byte frame each way twice a second, on a link
the roadmap already measures at 0.154% utilisation, which is not a cost.

### `hard_resync_threshold_us = 2000`

**Chosen from the two bounds it has to sit between**, and from the measured
steady-state above.

- **Above**: the largest steady-state error the loop shows in any modelled
  scenario is 184 us, so a threshold at 2 ms is nearly eleven times the error
  the fine tier actually holds. That margin is what keeps the loop out of the
  step tier during ordinary operation, which is AC-1's second half and AC-10's
  second half.
- **Below**: slewing an error away at the clamp takes `error / clamp` seconds,
  so 2 ms at 300 ppm is 6.7 seconds of audibly wrong playout. Past that, a
  muted splice is the smaller harm.

Not the 3 ms `ServoConfig::default()` carries. That value was chosen against a
1 ms modelled bound and is a quarter of the way to the ten-millisecond region
where a step is unarguable; against a 0.25 ms bound the same reasoning gives
2 ms.

### `max_correction_ppm = 300`

**Chosen from the skew it has to be able to cancel, and from the audible cost
of the correction.**

- **Above**: BRIEF.md section 6 puts endpoints at plus or minus 20 to 50 ppm
  each, so the worst realistic pair is 100 ppm apart. The clamp is three times
  that, which leaves the integral room to cancel the skew and still answer a
  transient.
- **Below**: a rate correction is frames inserted or dropped. 300 ppm at 48 kHz
  is 14.4 frames a second, or 0.3 ms of audio modified per second of playback.
  That is the number the clamp is really about, and it is why the clamp is not
  simply set wide.

This is **not** the 500 ppm `ServoConfig::default()` carries, which
`crates/sync/src/servo.rs` documents in as many words as "BRIEF.md 5.3's
reference constant" - another project's starting point. 300 ppm is what the
reasoning above gives; the modelled runs never asked for more than 100 ppm of
it, and the clamp was reported as biting in none of the three scenarios.

### `staleness_limit_ms = 10000`

**Chosen from the hard-resync threshold**, which is what makes it checkable
rather than a round number. Once the offset is stale the loop holds the
correction in force and computes no new one, so the error can walk at the
worst-case relative skew for as long as the limit allows. 100 ppm for 10
seconds is 1 ms, which has to stay under the 2 ms threshold or the loop would
step on the strength of an estimate it had already stopped trusting. 10 seconds
is also 20 exchange intervals, so a burst of loss does not trip it.

### `max_rtt_us = 100000`

**Chosen as a nonsense filter and not as a quality filter**, and it is worth
being explicit about which, because the two want very different numbers.
Quality filtering is the minimum-round-trip rule's job: a badly queued exchange
is admitted and then not selected. This ceiling is for exchanges that are not
merely queued but wrong.

The floor it has to clear: the loaded model's exponential jitter is capped at
ten times its 150 us mean, so a legitimate exchange can report a round trip of
3 ms, and throwing those away would be quality filtering by the back door. The
ceiling it must not exceed: half of it is the largest bound a client could
publish, and 50 ms is a hundred times the phase's 0.5 ms budget, so anything
near it is visibly useless rather than quietly wrong. 100 ms sits three orders
of magnitude above the wired round trip this phase is about.

### `playout_latency_us = 180000`

**Chosen from the client's existing buffer bounds**, which
`config/verification.conf` fixed in SOUND-2 and this phase does not move.

This is the fixed end-to-end latency every endpoint in a group applies: content
due at `t` is audible at `t + playout_latency_us` on the server timeline, at
every endpoint. It has to be the same at both endpoints or they are each
self-consistent and not aligned with each other, and it has to satisfy
`device_target_us < playout_latency_us < max_us`, which `ClientConfig::validate`
now checks and refuses.

At equilibrium the total occupancy the client holds equals this latency, split
between the queue and the device ring by the pacing: the ring holds
`device_target_us` (120 ms) and the queue holds the remaining 60 ms. Below the
device target there is no queue left to hold the difference; at or above
`max_us` (300 ms) the buffer is discarding what the loop is waiting for. 180 ms
is the midpoint of that band, which leaves 60 ms of queue and 120 ms of
headroom for a step in either direction.

### `mute_us = 20000`

**Chosen from the chunk duration**, which `config/verification.conf` fixes at
20000 us. The playout loop writes whole chunks, so a mute shorter than one
chunk would need a partial write to end in the right place and would buy
nothing: the splice it covers is a single sample boundary. The mute silences
frames in place and inserts none, so its length costs nothing in alignment and
is a purely audible choice.

### The gains: `kp = 0.4`, `ki = 0.08`, unchanged

**Inherited from `docs/decisions/0006-sync-simulator-and-servo.md`, and named
here so the inheritance is visible rather than assumed.** They were chosen
against the modelled clocks and this phase re-ran them against a modelled
device delay and a real correction mechanism; they hold the bound in all three
scenarios above with the window and cadence changed around them, so there was
no reason to move them and moving them would have made the sweep above two
sweeps.

## What changed in `crates/sync`, and why

The phase's brief says changing the servo's arithmetic is not required and any
change needs a reason here. **No arithmetic changed.** Two accessors were
added, both over state the code already computed:

- `OffsetFilter::selected()` returns the sample the last `push` chose out of
  the window. AC-4 requires publishing half the round trip **of the sample the
  offset came from**, and the filter is the only thing that knows which sample
  that was. The alternative was for the client to repeat the minimum-round-trip
  selection rule and hope the two spellings stayed equal, which is the drift
  this repository writes files like `config/sync.conf` to avoid.
- `Servo::last_correction_was_clamped()` returns whether the clamp cut the raw
  value down. AC-17 requires reporting that a correction was clamped, and the
  comparison was already being made inside `update` for the anti-windup branch.
  Inferring it by comparing the returned value against the clamp would be a
  second, weaker spelling of the same fact.

`Sample` became public so `selected()` has a return type.

## The server side

Two clients on one stream, which is the minimum that makes "one grouped stream"
real and no more than that. Zones, groups, naming, volume and discovery are
PRODUCT-6.

Before this phase the server accepted one connection at a time and built a
fresh `Chunker` on a fresh `MonotonicTimeline` for each. Two clients served
that way get two streams that happen to sound alike: there is nothing they are
both aligned TO. Now the chunks are cut once, on one timeline, and fanned out,
so the same content carries the same presentation timestamp at every attached
client. A client that attaches mid-stream gets a contiguous run from where it
attached - it does not start at sequence zero, because the stream did not.

The exchange shares that connection. The framing is self-delimiting and
`docs/protocol.md` already provides for a mix of types on one connection, so
nothing about the wire format changed. Two stamps make the reply worth having
and both are taken as late as they can be: `t1` when the request is decoded off
the socket, `t2` when the reply is encoded, after it has come off the outbound
queue. Stamping them together would fold the server's own queueing into the
network time the client measures, and the client would then correct for a delay
that is not there. `t3` stays zero on the wire: it is the client's receive
stamp on the client's clock, and the server will not invent it.

### `--serve-forever` still means what it says

`ServerConfig::once` predates this phase, `deploy/run-server.sh` and
`deploy/Dockerfile` both pass `--serve-forever`, and it is the deployed command
line. The accept loop it used to sit in is gone, so the flag is honoured in the
new shape instead: the acceptor thread runs for the life of the process and the
PRODUCER is what restarts, on a fresh source, once the last stream ended and a
new client has arrived. Waiting for that arrival is deliberate and is what the
old accept-then-serve loop did: nothing is produced into an empty room, and the
stream a listener joins starts where the audio does. `config.once` left unread
would have made a documented flag on the deployment path a silent no-op, which
is why `crates/server/tests/regress_0031_f1.rs` runs the real binary with
`--serve-forever` and asks a second client for a second stream.

**One timeline for the process, not one per stream.** The old server built a
fresh `MonotonicTimeline` for each connection, which cost nothing when a
connection and a stream were the same thing. They are not any more: clients stay
attached across a stream boundary. A fresh epoch would step their presentation
timestamps backwards while the `t1`/`t2` they are answered with came from the
epoch they attached on, which is two clocks inside one connection. Monotonic
time already gives the next stream a later origin than the last one's, so
nothing is bought by restarting it.

### The fanout has no back pressure, and therefore needs a ceiling

One endpoint that stops reading must not stop the stream the others are aligned
to, so `FanoutSink::write` never blocks the emitter on a slow client. That half
is right and is why the fanout exists. The other half is a bound: an unbounded
queue behind a stalled socket accrues about 192 kB a second at 48 kHz stereo
`pcm_s16le` in 20 ms chunks, without limit, in a process that has asked its host
for 64 MB of locked memory. `SUBSCRIBER_QUEUE_LIMIT = 128` items is 2.56 s of
audio at that shape and roughly 500 kB per stalled subscriber; a client that far
behind is past the ceiling its own buffer would drop at anyway. Items dropped
for a subscriber at its limit are COUNTED (`Fanout::dropped`) and reported on
the `stream done` line as `dropped_for_slow_clients`, because a drop nobody
counts is indistinguishable from a stream that was never sent.

## What this does not decide

- **Anything about hardware.** Every number above is modelled. AC-1 is the only
  criterion of this phase that measures two endpoints in a room, it needs a
  second endpoint and an audio interface this pipeline has no route to, and it
  is NOT passed. `tools/sync-hour-run.sh` is the committed entry point for it
  and refuses by name; `docs/verification-record.md` quotes the refusal.
- **Wireless.** WIFI-7 characterises that tier and is explicitly not held to
  the wired bound.
- **The C mirror.** The correction law and the filter are already pure
  functions in `crates/sync`, which is what EMBEDDED-5 will mirror; nothing
  added here changes what that mirror has to reproduce.
