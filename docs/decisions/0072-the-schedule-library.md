# 0072: the schedule library is a pure crate that reads no clock: civil time from TZif with a gap and a fold rule, weekly windows, alarms, sleep fades, integer ramps and generated chimes

- Status: accepted (goal 11, 2026-10-01)
- Decided by: the goal (section 15 items 2 and 3: quiet hours, alarms and sleep, "chimes
  (licences recorded)", volume ramps, sleep timers; K30, K80, K81); the coordinator's design
  envelope for goal 11 ("Schedule library"); the semantics below where the envelope left them open
- Implemented in: `crates/schedule` (`chorus-schedule`): `civil.rs`, `posix.rs`, `tzif.rs`,
  `zone.rs`, `window.rs`, `alarm.rs`, `sleep.rs`, `ramp.rs`, `chime.rs`; the exclusions in
  `audio-path.conf`; `fixtures/schedule/` and `tools/schedule-fixtures.sh`
  (`make schedule-fixtures`); held by `crates/schedule/tests/{tzif_fixtures,gaps_and_folds,chimes,no_clock}.rs`
  and the unit tests in each module
- The chimes: `docs/chimes.md`

## Context

Goal 11 brings alarms, sleep timers and quiet hours. All three are about the wall clock where the
listener lives ("07:00 on weekdays", "quiet from 22:00"), which BRIEF section 3.1 rule 4 keeps
off the audio path: monotonic clocks only there. K30 settles the split: the schedule is
server-owned, with a volume ramp, and the wall clock is used **for scheduling only, never in the
audio path**. The server needs a library it can call for "what time is it in this zone", "when
does this alarm next ring", "is this room in quiet hours", "what volume now, mid-ramp" and "what
does a chime sound like", that can be tested at any instant of any year, including the two
seconds a year daylight saving makes awkward. The runtime that calls it (firing, sources, the
`room_volume` sends) is the integration track's.

## What was read

All read 2026-10-01: RFC 9636, "The Time Zone Information Format (TZif)", October 2024
(https://www.rfc-editor.org/rfc/rfc9636.html), which obsoletes RFC 8536 (February 2019,
https://www.rfc-editor.org/rfc/rfc8536.html) and adds version 4; POSIX.1-2024 XBD section 8.3
(the TZ string), as RFC 9636 section 3.3 restates and extends it; the IANA time zone pages
(https://www.iana.org/time-zones, release 2026e of 2026-09-29;
https://data.iana.org/time-zones/tz-link.html, "the public-domain time zone database"); the
host's tzdata 2026c (`tzdata.zi`, its rule lines for the United States, the EU and Lord Howe, and
the Debian package's copyright file); "chrono-Compatible Low-Level Date Algorithms"
(https://howardhinnant.github.io/date_algorithms.html), for the day-number decomposition, written
here from its description; and in this repository `crates/control/src/*.rs` (idiom),
`crates/audio-path` and `audio-path.conf`, `crates/measure/tests/no_settable_wall_clock.rs`,
`crates/protocol/src/message.rs` (the sample formats and the rate and channel bounds),
`crates/client-linux/tests/codec_fixtures.rs` (the FNV-1a helper), `docs/conventions.md`,
`deny.toml`. timeanddate.com was tried as a cross-check for Lord Howe and refused the fetch (HTTP
403); glibc's `zdump` over the same tzdata stands in as the independent reader. No GPL source was
opened: zic and zdump were run, not read, and the tz code was not consulted.

## Decision

### A pure crate with no clock in it, and civil time allowed for scheduling only

`crates/schedule` has no socket, thread or file read, no external crate (no dependency at all;
`chorus-audio-path` is a dev-dependency for the clock check) and `#![forbid(unsafe_code)]`. **It
reads no clock of any kind.** Every function takes "now" as an argument: civil schedules take UTC
seconds since the Unix epoch (POSIX time), which the server reads from its settable clock outside
this crate; sleep timers and ramps take monotonic nanoseconds. `tests/no_clock.rs` holds every
unit to that with the audio-path check's own settable-clock vocabulary plus the monotonic
spellings, with red demonstrations. `audio-path.conf` lists every unit under `[excluded]` with its
reason, so if a unit on the path ever reaches this crate, the audio-path check already knows the
crate is scheduling and not the path. What crosses from here towards audio is a volume in
thousandths or PCM rendered ahead of time, never a civil instant.

### Civil time from TZif, versions 1 to 4

The server reads `/etc/localtime` or `/usr/share/zoneinfo/$TZ` and hands the bytes to
`Zone::from_tzif`; `zone::is_safe_zoneinfo_name` refuses a `TZ` name that would escape the
zoneinfo directory (`..`, absolute, empty components). Version 1 files are read from their only
block; version 2 to 4 files from the 64-bit block and the footer. Past the last transition the
footer's POSIX TZ string answers (`Jn`, `n` and `Mm.w.d` dates, rule times of -167 to 167 hours
as version 3 allows, accepted whatever the version since it is a superset). Every count, index,
offset and ordering is checked; a damaged file is a typed error, never a panic (a test truncates
a file at every length and flips every byte). Refused by policy: a file with leap-second records
(a `right/` zone counts leap seconds and would put every alarm about 27 s off POSIX time), a UT
offset outside -89 999 to 93 599 s (RFC 9636 section 3.2), and a TZ string with a daylight-saving
name and no rule (POSIX leaves the rule implementation-defined; a guessed rule is a wrong alarm
twice a year). The fallbacks: `Zone::utc()`, `Zone::fixed(offset)`, and `Zone::from_posix` for a
`TZ` value that is a rule rather than a name.

### The gap and fold rules

The inverse conversion (local date and time of day to a UTC instant) is computed exactly, by
cutting the two days either side into constant-offset segments; `Zone::resolve` reports
`Unique`, `Fold { first, second }` or `Gap { skipped_to }`.

- **Gap**: a local time clocks skip resolves to **the first valid instant after it**, the
  transition instant itself. A 02:30 alarm on the US spring-forward morning rings at 03:00 EDT,
  late by the jump and never skipped; Samoa's skipped Friday (2011-12-30) rings a Friday alarm at
  the jump to Saturday 00:00.
- **Fold**: a local time that happens twice resolves to **its first occurrence**, and rings
  **once**: `next_local_after` never offers the second reading, even when asked between the two.

### Weekly windows (quiet hours)

A days mask, a start and an end time of day. Each masked day opens an instance at `start` that
lasts until the clock next reads `end`; `end < start` crosses midnight and belongs to the day it
**starts** on; `start == end` is **a full 24 hours** from `start` (so an instance is never empty;
"never" is the empty mask). The window is about wall-clock readings, so `22:00-07:00` ends at
07:00 on the night clocks change, and in a fold it follows the clock both times round.
`contains` answers for a civil time; `next_boundary_after` returns the first instant the answer
changes, from every reading of a boundary, every gap resolution and every transition in nine days
(a test walks a week minute by minute in three zones and finds the changes exactly there, and
nowhere else).

### Alarms

Time of day, days mask, enabled. A repeating alarm rings at the next reading of its time on one
of its days by the rules above; a one-shot alarm (the empty mask) rings at the next reading on any
day and `fired` disables it. Snooze is included: `snooze(now, minutes)` rings again that much
elapsed time later (not a wall-clock reading), a pending snooze rings even after a one-shot alarm
disabled itself, and `stop` or disabling clears it. The server polls `due_between(alarms, zone,
t0, t1)` over the half-open interval `(t0, t1]`: each alarm at most once, at its first ring
instant there, so consecutive polls neither miss nor repeat, and a server that was down gets each
missed alarm once with the instant it should have rung and decides itself whether to ring late.

### Sleep timers and their fade

A sleep timer is monotonic: it expires at `start + minutes` of elapsed time (0 minutes cancels).
The room is silent by expiry: over the 30 s before it (or from the start, for a shorter timer)
the volume ramps from the value it had when the fade began to 0, and at expiry the stream stops
and the room's volume is **restored** to that pre-fade value, so the next thing played starts
where the person left it. `SleepTimer::step(now, volume_at_fade_start)` returns `Waiting`,
`Fading { volume }` or `Stop { restore }`.

### Ramps

`Ramp { from, to, duration }`, sampled at a monotonic elapsed time: `from + (to - from) * elapsed /
duration` in 128-bit integer arithmetic, truncated toward `from`. Exactly `from` at 0, exactly `to`
at and after the end, monotone between (truncating a monotone quotient keeps it monotone), linear
in amplitude like `room_volume`'s `ramp_ms`. A 2 000-ramp seeded property test samples 64 points
of each. The alarm ramp is `alarm_ramp(target, ramp_s)`, 0 to the target.

### Chimes, and their licence

Three named chimes, `bell`, `ding-dong` and `triad` (`docs/chimes.md` describes each), generated
by additive synthesis: per strike, partials with exponential decay, a short linear attack so a
strike does not click, a final linear release reaching exactly zero, normalised to a peak of
exactly 0.5 of full scale (-6.02 dBFS). Rendered to interleaved `pcm_s16le`, `pcm_s24le` (packed)
or `pcm_f32le` at 8 to 384 kHz on 1 to 8 identical channels, the wire's own bounds. Deterministic
to the bit on every machine: the phase is integer millihertz arithmetic, and the sine and the
decay are computed from IEEE 754 basic operations (no libm call, which may differ in the last bit
between platforms). The 48 kHz mono s16le render of each is pinned by an FNV-1a 64 digest.
**Licence: chorus's own generated sound, MIT OR Apache-2.0; no third-party sample is used, so
nothing is attributed.**

### Every ASSUMED default

| Value | Where | Why ASSUMED |
|---|---|---|
| Alarm ramp 30 s | `ramp::DEFAULT_ALARM_RAMP_S` | a gentle default, nothing measured |
| Sleep fade 30 s | `sleep::SLEEP_FADE_S` | long enough not to wake with a cut, short enough to be done when promised |
| Snooze 9 min | `alarm::DEFAULT_SNOOZE_MIN` | the common clock-radio and phone default; not measured |
| Snooze and sleep at most 24 h | `alarm::MAX_SNOOZE_MIN`, `sleep::MAX_SLEEP_MIN` | overflow bounds, not usability claims |
| Chime peak -6.02 dBFS (ceiling -6 dBFS) | `chime::PEAK` | a chime over music must not be the loudest thing in the room by surprise |
| Chime attack 5 ms, release 300 ms | `chime::ATTACK_MS`, `chime::RELEASE_MS` | by ear in principle; not measured |
| `start == end` is 24 h | `window` | a decision, recorded above |

## Not chosen

- **An external time zone crate** (chrono-tz, jiff, tz-rs): a time zone database compiled into
  the binary goes stale with every tz release, while the host's zoneinfo is updated by its
  package manager; and the reader is a few hundred lines of a stable RFC, small and instructive
  (BRIEF section 3.2). Reading the host's files is the fitness argument.
- **Firing a skipped alarm at the same offset ("02:30 EST" = 03:30 EDT), or not at all.** The
  first is what some libraries do for a gap; it rings later than necessary for no benefit. Not
  ringing at all is the one answer an alarm must never give.
- **Firing a folded alarm twice, or at the second reading.** Twice is a second wake-up nobody set;
  the second reading is an hour later than the person expected on the one morning it matters.
- **`start == end` as an empty window.** An instance nobody can see is a configuration error that
  looks like a setting; the empty mask already says "never".
- **A sleep fade that starts at expiry.** Then "stop in 30 minutes" plays for 30.5; the fade ends
  at expiry instead.
- **Chimes from libm sine and exponential.** Simpler, but a golden digest could differ between the
  build host and an aarch64 endpoint package.
- **Leap-second-aware TZif.** chorus's instants are POSIX time; a `right/` file is refused by name.

## Follow-ups

- The integration track wires the runtime: reading the zoneinfo (`TZ`, then `/etc/localtime`,
  then UTC), polling `due_between`, the sleep plan per room, quiet windows into the effective
  limit, `alarm_ramp` into `room_volume`'s `ramp_ms`, and chimes as a slot source.
- The owner listens to the three chimes (`--example chime_wav`); a change of sound is a changed
  digest and a line in `docs/chimes.md`.
- A zoneinfo change while the server runs (a tzdata upgrade) is re-read at the server's next
  start; watching the file is the integration track's call.
