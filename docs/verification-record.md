# The verification record

Which criterion each committed check answers, what actually ran when this work
was built, and - for the checks whose environment was not there - the exact
command, the prerequisite that was missing, the verbatim refusal, and where the
instructions are for whoever can run it.

This file is committed inside the repository on purpose. A record of what was
and was not run is only useful to a reader who can read it, and a reader who
picks up this tree later has no access to the notes the implementing session
wrote elsewhere. If the two ever disagree, this file is the one attached to the
code.

Two phases are recorded here, most recent first:

- [RIG-3, the measurement harness](#rig-3-the-measurement-harness)
- [SOUND-2, first sound](#sound-2-first-sound)

## RIG-3: the measurement harness

### The machine this was written on

```
kernel                    Linux 6.12.90+deb13.1-amd64 x86_64
toolchain                 rustc 1.98.1 (48a229cea 2026-09-01)
/dev/snd                  absent - no sound card of any kind
libasound.so.2            present, and the ALSA `null` PCM opens for playback
capture device            none. There is no audio interface of any kind here
ulimit -r (RLIMIT_RTPRIO) 0
container                 a disposable Linux container, not the deploy target
```

**This machine has no capture device, so no capture was ever taken here.** Every
figure below is from a committed synthetic capture whose ground truth is stated
in the `.params` file beside it. That is the whole design of this phase, not a
compromise in it: the roadmap phase wants a claim someone else can check, and an
analysis that could only run against a live device could not be checked by
anyone without two endpoints and an interface.

### What ran here

| criterion | what answers it | result |
|---|---|---|
| AC-3 | `cargo test -p chorus-measure --test report_shape` | a completed run writes exactly one new file under `docs/measurements/`; the two committed reports are there, each naming a 40-character commit, and the lag report cites the baseline the free-run report established |
| AC-4 | `cargo test -p chorus-measure --test lag_resolution` | against `fixtures/measure/01-chirp-pair-a.wav`, whose parameters declare 254.0 us (24.384 samples at 96 kHz): median +253.981 us, p95 254.026 us, maximum 254.039 us. Largest error 0.039 us against a 10 us budget, over 32 of 32 windows |
| AC-5 | the same target | fixtures 01 and 02 are 10.0 us apart, which is 0.96 of a capture sample; their medians differ, and differ by 10.0 us to within 1 us. A whole-sample estimator reports one number for both |
| AC-6 | the same target | 01 reports +253.981 us and "channel A leads channel B"; 03, which is 01 with the channels exchanged and nothing else changed, reports the same magnitude negated. The convention is declared in `chorus_measure::lag::SIGN_CONVENTION` and printed in every report |
| AC-7 | `cargo test -p chorus-measure --test free_run_slope` | the noiseless series fits +37.5000 ppm against a declared 37.5 ppm with zero residual; the jittered one, at the 20 us RMS its fixture states and which the residuals measure back to within 15%, fits inside the 5 ppm budget |
| AC-8 | `--test report_shape` | `docs/measurements/free-run-baseline.conf` carries the measured slope, the report that established it and the commit that report was written against; the committed lag report names both the value and that report. `source = fixture` is carried through, so nobody reads it as a statement about a crystal |
| AC-9 | `--test report_shape` | one new file, carrying the commit, whether the tree was clean, 96000 Hz, "32 offered, 32 used", and all three lag figures. Both states are in the committed reports: the free-run one was written from a clean tree and says so, and the lag one, written second, names the two uncommitted paths the free-run one had just created. A dirty tree is asserted separately so a report that always said "clean" would fail |
| AC-10 | `--test report_shape` | all 13 committed fixture inputs regenerate from their committed parameters byte for byte, and a second analysis over the same capture returns an identical `LagSummary` and an identical figures table |
| AC-11 | `cargo test -p chorus-measure --test no_settable_wall_clock` | every one of the crate's 11 units is accounted for in `audio-path.conf`; none of the listed ones reads a settable clock; a read smuggled into `freerun.rs` or `lag.rs` turns the scan red and names the file; and the report writer is the ONLY unit of the crate that reads one, which the exclusion cannot assert about itself |
| AC-12 | `make check` | `cargo build --workspace --all-targets` then `cargo test --workspace`, both clean, on this machine with no audio device and no privilege. 40 test binaries, 0 failures |
| AC-13 | `cargo test --workspace --offline` | passes. Every dependency in the workspace is a `path` dependency on another crate in it; `Cargo.lock` is still untracked and still ignored, and `docs/decisions/0002` is unchanged |
| AC-14 | `--test refusals`, and `make verify` through the real binary | silence, full-band noise and the reversed-chirp pair hit three DIFFERENT named conditions (`silent-channel`, `no-chirp-present`, `below-confidence-floor`), each exits non-zero, each names its condition, none reports a lag, and `docs/measurements/` holds the same 5 files before and after |
| AC-15 | `--test refusals`, and `make verify` | `--amplitude 0.9` exits 4 with "requested: 0.9 full scale", "permitted: 0.25 full scale, declared in config/measure.conf" and "emitted: nothing. No device has been opened." It refuses BEFORE the device probe, asserted by the absence of a probe line. Exactly the ceiling is permitted; a hair over it, and NaN, are not |
| AC-16 | `--test refusals`, and `make verify` | a mono capture ("1 channel(s) and 2 were required"), a float one ("WAVE_FORMAT_IEEE_FLOAT at 32 bits ... WAVE_FORMAT_PCM at 16 bits was required"), a truncated one ("declares 7680 bytes and 3956 bytes are present"), and a rate the run did not declare. Each is a typed error at start and none writes a report |
| AC-17 | `--test free_run_slope` | the 21-observation series refuses as `series-too-short` naming the 30 it required; the 3 ms-jitter series refuses as `series-too-noisy` and says what it withheld. With the bound removed the same series publishes a number wrong by more than the bound allowed, which is what the bound is for |
| AC-18 | `--test refusals` | a missing destination, a destination that is a file, and a destination under a file are each refused naming the path and the reason. The write is a temporary file plus a rename, so a refused write leaves nothing behind at all |
| AC-19 | `make verify` | `tools/measure/capture-run.sh` exits 3 naming the prerequisite and the criterion; `tools/unrun-checks-are-visibly-unrun.sh` now derives its list recursively from `tools/`, finds 9 environment-dependent entry points and checks all 9. See the refusal quoted below |

### AC-19 in full: the device-backed entry point, refusing here

This is the row that matters most on a machine with no interface, so it is
quoted rather than summarised. `make verify` runs it, and this is what it
printed here:

```
--- capture-run.sh (exit 3)
    chorus: the device-backed measurement run
    MISSING PREREQUISITE
      criterion:    two endpoint line outputs captured together, cross-correlated into median, p95 and maximum inter-device lag at 10 us or better
      prerequisite: an ALSA capture device that opens two channels at the declared rate; 'chorus-no-such-capture-device' does not (capture-device-probe device=chorus-no-such-capture-device usable=0 detail=snd_pcm_open on audio device 'chorus-no-such-capture-device' failed: No such file or directory (errno 2))
      how to get it: connect both endpoints' line outputs to the L and R inputs of one audio interface and point CHORUS_CAPTURE_DEVICE at it
      this check is NOT passed, NOT skipped-green and NOT satisfied.
pass capture-run.sh refuses, names both, and claims nothing
```

The same shape every other environment-dependent entry point in this repository
uses, for the reason `tools/lib.sh` gives at the top of itself. A capture run
that reported green on a machine with no capture device would make every row
above worthless.

### What did NOT run here, and is not claimed

#### AC-1 and AC-2: `[grade: operator]`

These two are the roadmap phase's own criteria, adopted verbatim, and in that
form each describes a run against physical hardware:

- **AC-1.** WHEN the harness captures two endpoint line outputs playing a shared
  chirp THE SYSTEM SHALL report median, p95 and maximum inter-device lag at a
  resolution of 10 us or better.
- **AC-2.** WHEN two clients run with correction disabled THE SYSTEM SHALL
  report the slope of their relative offset in ppm and SHALL record it as the
  free-run baseline later runs are compared against.

**Neither is passed here and neither is claimed.** They need two endpoints, an
audio interface and real loudspeakers; this machine has none of those and this
pipeline has no route to acquire them. AC-4 through AC-9 discharge every
OBSERVABLE inside them against committed fixtures with known ground truth, which
is what an operator can then point at hardware - but a fixture is not a
loudspeaker and this record will not pretend otherwise.

What an operator with the environment runs:

```
CHORUS_CAPTURE_DEVICE=hw:1,0 CHORUS_CLIENT_DEVICE=hw:0,0 \
    ./tools/measure/capture-run.sh 30          # or: make verify-measure-device
```

Instructions: the script's own header, and
`docs/decisions/0013-the-measurement-rig.md` for what every declared threshold
means. The capture it writes IS the evidence and can be analysed afterwards, on
any machine, by someone who did not take it:

```
cargo run -p chorus-measure --bin chorus-measure -- lag <capture.wav> --label <name>
```

For AC-2, the free-run half, an operator runs two clients with correction
disabled, logs their relative offset, and fits it:

```
cargo run -p chorus-measure --bin chorus-measure -- free-run <series.offsets> \
    --label <name> --source hardware
```

`--source hardware` is the load-bearing word. The committed baseline in
`docs/measurements/free-run-baseline.conf` says `source = fixture`, and both the
artifact and every report that cites it say in as many words that a
fixture-derived baseline bounds the ESTIMATOR and is not a statement about any
real crystal. The day a hardware baseline is recorded, that word changes and the
sentence goes away.

#### What a committed fixture cannot establish

Stated plainly because the numbers above look precise enough to be mistaken for
something they are not. An error of 0.039 us against a synthetic capture says
the estimator is not what spends the 10 us budget. It says nothing about:

- what two amplifiers, two loudspeakers and a room spend of it;
- whether a real capture interface's two channels are sample-aligned with each
  other, which is an assumption every figure here rests on;
- what a real endpoint's line output actually does at the chirp's band edges;
- the drift of any real crystal, which is AC-2 and is operator graded.

### Running the lot, on a machine that has the environment

```
make test                    # the suite: no device, no privilege
make verify                  # every refusal path, including the rig's
make measure-fixtures        # regenerate the committed fixture inputs
make measure-fixture-reports # the saved reports over those fixtures
make verify-measure-device   # the capture run: needs two endpoints and an interface
```

## SOUND-2: first sound

### The machine this was written on

```
/dev/snd                  absent - no sound card of any kind
libasound.so.2            present, and the ALSA `null` PCM opens
ulimit -r (RLIMIT_RTPRIO) 0
ulimit -l (RLIMIT_MEMLOCK) 8 MiB, below the 64 MiB the server asks for
```

So: everything that needs no device and no privilege ran; everything that needs
only a device that OPENS ran against `null`; nothing that needs a device that
paces, a real-time ceiling, or a device that can be pulled out mid-run ran, and
none of it is claimed.

### What ran here

| criterion | what answers it | result |
|---|---|---|
| AC-1 | `cargo test -p chorus-audio` (chunker), and over a real TCP socket `-p chorus-server --test serving` | ten chunks plus 100 frames plus three bytes gives eleven chunks, only the last short and marked final, concatenation byte-identical to the input |
| AC-2 | the same capture | the sequence column is a contiguous run |
| AC-3 | the same capture | strictly increasing, constant 20 ms start-to-start delta including across the short final chunk |
| AC-4 | `cargo test -p chorus-protocol` | the two FOUNDATION-1 vectors are untouched; the new type has its own committed vector; the live capture decodes with the committed decoder |
| AC-5 | `tools/start-fill-and-log-shape.sh` on the ALSA `null` device, through the real binaries | the first write carried 120000 us of a configured 120000 us fill; the log names it; no sample before it is graded |
| AC-7 | the same run | 594 samples in 60 s, all four columns on every one, the widest gap 102106 us, one config record carrying the bounds and start fill |
| AC-9, AC-14, AC-24 | `-p chorus-client-linux --test playout`, against the modelled device described in `tests/common/mod.rs` | the crossing is reported, the overflow counter is its own, occupancy never exceeds max plus one chunk, a stalled feed moves the underrun counter without widening a bound. **A model is not a sound card**; `tools/overflow-run.sh` is the run on a device that paces |
| AC-15 | `-p chorus-audio`, and over TCP | `bytes_discarded=3`, eleven chunks, sequence still contiguous |
| AC-16 | `make verify` (`tools/refusals.sh`) | exit 2, the format named, `chunks_sent=0`; same for an unsupported rate |
| AC-17 | `-p chorus-server --test serving`, on the chosen transport | a truncated chunk is a typed framing error and never becomes audio; a duplicate and a malformed frame are counted by reason with the session open and the underrun counter unmoved |
| AC-18 | `-p chorus-client-linux --test playout` | `discarded_late=1`, the chunk that was due is not displaced |
| AC-19 (first half) | `make verify` | exit 4, the device and the reason named, `usable=0 paces=0` |
| AC-20, AC-21 | `make verify-null-device` (`tools/stream-end-and-loss.sh` on `null`), through the real binaries | clean end: exit 0, in-band signal after final sequence 100, 96480 frames sent and 96480 written, `underruns=0`. Lost server: exit 3, `reason=connection-lost`, `played=1`, `underruns=0` |
| AC-22 (ceiling-zero half) | `make verify` | exit 3, both numbers named, nothing played; with the option, it starts and 4 of 4 status reports say so |
| AC-23 | `cargo test -p chorus-audio-path` | the committed tree passes both audio-path checks; both red demonstrations go red, in every module-declaration spelling. The same crate's `--test real_time_ordering` is a separate invariant, recorded under the host contract below |
| AC-25 | `make verify` (`tools/unrun-checks-are-visibly-unrun.sh`) | all environment-dependent entry points exit non-zero naming prerequisite and criterion, and the list is derived from the tools rather than restated. It was 8 when this was written and is 9 since RIG-3 added `tools/measure/capture-run.sh`, which is the derivation doing its job |
| AC-26 (denial half) | `make verify` | exit 3, the limit read and the amount wanted both named, nothing played; with the option, 4 of 4 status reports say so |
| AC-27 (start-up half) | `tools/start-fill-and-log-shape.sh` on `null`, and `make verify` | the three relations hold against the recorded config line: `min_us=60000 > 0`, `60000 < start_fill_us=120000 < 300000`, span 240000 us at 2000 ppm crosses in 120 s; a configuration that would not cross is refused at start |

**What the `null` device does not buy.** It accepts every frame instantly and
reports a delay of zero forever, so every sample in that one-minute log carries
`delay_us=0`. The run above is evidence about the start fill, the record's
shape and the bound relations, and about nothing whatever concerning the VALUE
of a device-reported delay. That is why `tools/delay-log-shape.sh` and
`tools/ten-minute-run.sh` refuse `null` by name.

### What did not run here, and what is filed instead

Every row below is a committed entry point that exits non-zero, naming the
prerequisite and the criterion, rather than reporting green. The refusals are
quoted verbatim from a run in this container.

#### AC-6, AC-8, AC-27 (the run-completion half): the ten-minute run

```
./tools/ten-minute-run.sh docs/measurements/ten-minute-run.log
```
with `CHORUS_CLIENT_DEVICE` pointed at real speakers. Missing here: an ALSA
device that reports a delay.

```
MISSING PREREQUISITE
  criterion:    ten continuous minutes with the reported delay inside its bounds and zero underruns
  prerequisite: an ALSA playback device that reports a delay; 'chorus-no-such-device' does not (...)
  how to get it: point CHORUS_CLIENT_DEVICE at a real card or a snd-aloop loopback; the ALSA 'null' device is not one, because it accepts every frame instantly and reports a delay of zero
  this check is NOT passed, NOT skipped-green and NOT satisfied.
```

Instructions: `docs/sound-2.md`, "The ten-minute run". The log it writes IS the
evidence and can be graded afterwards, on any machine, by someone who did not
run it:

```
./target/debug/chorus-delaylog-check <log> --min-graded-seconds 600 \
    --require-zero-underruns --require-no-rate-change
```

#### AC-5, AC-7 graded against a real delay, and AC-9, AC-14, AC-24 on a device

```
./tools/delay-log-shape.sh          # one minute, graded the way the long run is
./tools/overflow-run.sh             # the over-rate run, to the ceiling
```

Both need a device that paces; the refusal has the same shape as the one above,
with their own criterion lines. AC-5, AC-7 and AC-27's start-up half are
already answered above on `null`; what these two add is everything that rests
on the delay a device actually reports. Instructions: `docs/sound-2.md`, "The
client" and "The values this phase chose".

#### AC-10, AC-11, AC-12, AC-13, AC-26 (the locked half): the host contract

```
./tools/host-contract.sh
./tools/spin-test.sh
```

Missing here: a granted rtprio ceiling above zero.

```
MISSING PREREQUISITE
  criterion:    a real-time thread that runs without yielding past its CPU-time limit is terminated by that limit within one second, and a normal-priority process beside it keeps making progress
  prerequisite: a granted rtprio ceiling above zero; RLIMIT_RTPRIO reads 0 here
  how to get it: run in a container started with 'docker run --ulimit rtprio=<n>', as deploy/run-server.sh does
  this check is NOT passed, NOT skipped-green and NOT satisfied.
```

Instructions: `deploy/README.md`, "Checking the contract holds", and
`docs/sound-2.md`, "The spin test". What did run here: `-p chorus-hostctl`,
which covers the limits being read, `/proc/self/task` parsing, the
no-undeclared-real-time-thread check and both denial paths; and, in `make
test`, `cargo test -p chorus-audio-path --test real_time_ordering`, which
grades the CPU-time-bound ordering invariant against the committed
`real-time-acquisitions.conf`: every real-time acquisition site in the tree
applies `RLIMIT_RTTIME` earlier in the same function than it takes the
scheduling policy, an acquisition in a unit the file does not name is reported
as unaccounted for rather than skipped, and the three demonstrations go red for
an acquisition taken before its bound, for an unlisted acquisition, and never
for a name occurring only in prose.

That is a source scan and not a run of the scheduler, which is why it needs no
ceiling and appears here rather than in the rows above; what a granted ceiling
would add is `tools/spin-test.sh`, which is the entry point that makes the
bound fire.

##### What the suite now covers here with no privilege, and what still needs a ceiling

AC-13 is "no thread is real-time without being reported", and that answer is
only as good as the thread inventory it is read out of. The inventory is now
either complete or an error, and each way it can fail has its own test that
needs no privilege, no sound card and no real fault: the failure is injected at
the seam the enumeration reads the kernel through, with the errno the kernel
would have returned. **Covered here, by `cargo test -p chorus-hostctl`**
(`crates/hostctl/tests/thread_inventory.rs`, nine tests):

| behaviour | the test that holds it |
|---|---|
| one entry per live thread, carrying tid, name, policy and real-time priority, checked against a modelled listing, against a listing that LOSES a live thread on one pass (which `/proc/<pid>/task` was measured doing), and against this process's real `/proc/self/task` with four threads held alive on a barrier | `inventory_lists_every_live_thread` |
| a thread that exited between the listing and the read is omitted, the run still succeeds, and the omission is COUNTED so a caller can tell it from "nothing was dropped" | `vanished_thread_is_omitted_and_counted` |
| any other read failure fails the whole enumeration naming the thread and the reason, instead of shortening the list | `transient_read_failure_is_an_error_not_an_omission` |
| a refusal to list the threads, or to read one of them, fails the enumeration and hands no caller a partial list | `permission_denial_fails_the_enumeration` |
| a record that cannot be interpreted fails the enumeration naming that thread, rather than reading as a thread that is not there | `unreadable_scheduling_record_is_an_error` |
| an empty record, and a listing entry that names no thread, fall under the same rule and are never reported with default or zero values | `an_empty_record_is_never_reported_as_defaults` |
| an inventory with nothing in it is an error, because the thread doing the asking is itself a thread | `an_empty_inventory_is_an_error` |
| a name that cannot be read costs the thread its name and not its place, and is never the empty string | `an_unreadable_thread_name_does_not_drop_the_thread` |
| the undeclared-real-time-thread question is answered only from an inventory that completed, and an incomplete one is reported as the failure it is rather than as zero | `no_clean_answer_from_an_incomplete_inventory` |

Also covered here, by `make verify` (`tools/refusals.sh`): the host-contract
entry point, handed a report whose inventory did not complete, exits non-zero,
names this criterion and the reason, and reports nothing as passed or
skipped-green (`incomplete-inventory-*` checks), while still grading a complete
report on its merits (`complete-inventory-still-grades-clean`). Repeat-run
evidence for the suite is in
`docs/measurements/hostctl-thread-inventory-repeat.md`, which records 30
consecutive clean runs of `make test`, a 200-run before-and-after comparison
that took a real flake in `-p chorus-hostctl --lib` from 3.5% to zero observed,
the captured diagnosis of what was causing it, and a plain statement that a
clean streak bounds a flake rate rather than proving a flake gone.

**Still needs a granted rtprio ceiling above zero, and is NOT covered here.**
Everything about a thread that is actually real-time on this machine, because
no thread here can become one:

- that the priority obtained sits inside the granted ceiling, and is the
  running thread's actual scheduling priority (AC-10);
- that every real-time thread the server created is reported carrying a
  CPU-time bound, and that the bound FIRES rather than merely being configured
  (AC-11, AC-12, `tools/spin-test.sh`);
- that the count of real-time threads in the report matches the count the
  process actually has, against a process that has one (AC-13's positive half).
  The tests above establish that the inventory feeding that count is complete
  or is a refusal; they do not and cannot establish what the count is on a host
  that grants a ceiling.

`tools/host-contract.sh` remains the entry point for all of those and remains
in `tools/unrun-checks-are-visibly-unrun.sh`, so it still refuses visibly here.

#### AC-19 (the second half): a device removed mid-run

```
CHORUS_REMOVABLE_DEVICE=hw:Loopback,0 \
CHORUS_REMOVE_COMMAND='sudo modprobe -r snd_aloop' \
./tools/device-loss-run.sh
```

Missing here: a device that can be removed or made unusable mid-run.

```
MISSING PREREQUISITE
  criterion:    a device that becomes unusable during a run is reported with its reason, the client exits non-zero, and it never reports itself as playing while producing no audio
  prerequisite: a device that can be removed or made unusable mid-run
  how to get it: set CHORUS_REMOVABLE_DEVICE to an ALSA device you can unplug or unbind, and CHORUS_REMOVE_COMMAND to the command that removes it
  this check is NOT passed, NOT skipped-green and NOT satisfied.
```

Instructions: the script's own header. Nothing here guesses how to break a
device on someone else's machine. What did run here: the mid-run sink failure
against the modelled device, which exercises the client's half of it.

### Running the lot, on a machine that has the environment

```
make test                 # the suite: no device, no privilege
make verify               # the refusal paths, and that unrun checks are visibly unrun
make verify-null-device   # the device checks that do not grade the delay
make verify-device        # everything that needs a device (paces, for three of the four)
make verify-host          # the scheduling contract and the spin test
make ten-minute-run       # the evidence run
```

Each one either does the work or exits non-zero saying what it lacks. None of
them reports green for a check it did not run.
