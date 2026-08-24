# The verification record for SOUND-2

Which criterion each committed check answers, what actually ran when this work
was built, and - for the checks whose environment was not there - the exact
command, the prerequisite that was missing, the verbatim refusal, and where the
instructions are for whoever can run it.

This file is committed inside the repository on purpose. A record of what was
and was not run is only useful to a reader who can read it, and a reader who
picks up this tree later has no access to the notes the implementing session
wrote elsewhere. If the two ever disagree, this file is the one attached to the
code.

## The machine this was written on

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

## What ran here

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
| AC-23 | `cargo test -p chorus-audio-path` | the committed tree passes both checks; both red demonstrations go red, in every module-declaration spelling |
| AC-25 | `make verify` (`tools/unrun-checks-are-visibly-unrun.sh`) | all 8 environment-dependent entry points exit non-zero naming prerequisite and criterion, and the list of 8 is derived from the tools rather than restated |
| AC-26 (denial half) | `make verify` | exit 3, the limit read and the amount wanted both named, nothing played; with the option, 4 of 4 status reports say so |
| AC-27 (start-up half) | `tools/start-fill-and-log-shape.sh` on `null`, and `make verify` | the three relations hold against the recorded config line: `min_us=60000 > 0`, `60000 < start_fill_us=120000 < 300000`, span 240000 us at 2000 ppm crosses in 120 s; a configuration that would not cross is refused at start |

**What the `null` device does not buy.** It accepts every frame instantly and
reports a delay of zero forever, so every sample in that one-minute log carries
`delay_us=0`. The run above is evidence about the start fill, the record's
shape and the bound relations, and about nothing whatever concerning the VALUE
of a device-reported delay. That is why `tools/delay-log-shape.sh` and
`tools/ten-minute-run.sh` refuse `null` by name.

## What did not run here, and what is filed instead

Every row below is a committed entry point that exits non-zero, naming the
prerequisite and the criterion, rather than reporting green. The refusals are
quoted verbatim from a run in this container.

### AC-6, AC-8, AC-27 (the run-completion half): the ten-minute run

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

### AC-5, AC-7 graded against a real delay, and AC-9, AC-14, AC-24 on a device

```
./tools/delay-log-shape.sh          # one minute, graded the way the long run is
./tools/overflow-run.sh             # the over-rate run, to the ceiling
```

Both need a device that paces; the refusal has the same shape as the one above,
with their own criterion lines. AC-5, AC-7 and AC-27's start-up half are
already answered above on `null`; what these two add is everything that rests
on the delay a device actually reports. Instructions: `docs/sound-2.md`, "The
client" and "The values this phase chose".

### AC-10, AC-11, AC-12, AC-13, AC-26 (the locked half): the host contract

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
no-undeclared-real-time-thread check and both denial paths; and a grep-checked
invariant that both real-time acquisitions in the tree apply the CPU-time bound
before anything else.

#### What the suite now covers here with no privilege, and what still needs a ceiling

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

### AC-19 (the second half): a device removed mid-run

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

## Running the lot, on a machine that has the environment

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
