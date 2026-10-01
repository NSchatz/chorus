# 0092: a volume or a mute keeps a room in its autoplay, one stereo speaker folds a surround stream by BS.775, two test flakes are fixed at their causes, and bench session S8 measures the TV path

- Status: accepted (goal 13, 2026-10-01)
- Decided by: the goal-13 coordinator's follow-ups for track `tv-fixes` (from ADRs 0087, 0088,
  0090 and 0091); ADR 0076's alarm rule, kept; BRIEF.md section 3.1 (measurement rule) and
  section 5.7 (the lip-sync calibration); the placements below where the follow-ups left them open
- Implemented in: `crates/server/src/schedule_runtime.rs` (`touched`, `is_level_change`);
  `crates/client-linux/src/dsp.rs` (`stereo_fold`, `DspSink`), `crates/client-linux/src/main.rs`
  (the two-channel reopen); `crates/client-linux/tests/line_in_source.rs`;
  `tools/bench/validate-report.sh`, `tools/bench/lib.sh`, `tools/bench/e2e-test.sh`;
  `crates/hostprobe/src/udploss.rs`, `crates/hostprobe/src/bin/chorus-udp-loss.rs`,
  `audio-path.conf`; `docs/bench-packet.md` (S8), `docs/control-plane.md`, `docs/dsp.md`,
  `docs/linux-endpoint.md`; `docs/research/research-tv-path.md`. Held by
  `crates/server/tests/schedule_runtime.rs` (3 new tests, 1 changed), `crates/server/tests/cec_tv.rs`
  (extended), the unit tests in `dsp.rs` (2) and `udploss.rs` (5), and the bench e2e's new repeat
  check

## Context

Goal 13's first tracks left five loose ends. The hub's CEC volume keys reach the server as a
person's `controller_command` (ADR 0087), and the schedule runtime let any person's command on a
room detach it from its autoplay, so a TV turned down and then switched off stopped nothing.
`EndpointDsp::stereo_downmix` (ADR 0088) had no setter. Two tests failed intermittently in gates:
`line_in_source`'s "the exchange ran on the shared writer", and the bench e2e's
`validate-open-prs` strict count. And the TV path's `ASSUMED` values (ADRs 0087 to 0091) had no
bench session to replace them.

## What was read

All read 2026-10-01: ADRs 0076, 0087, 0088, 0090, 0091; the goal-13 design envelope and research
notes (cec, fec-latency, capture), merged here as `docs/research/research-tv-path.md` with every
citation they carry; `docs/proposals/P2-theater-scope.md` (its 2026-09-30 re-check and source
list, for the S8 buy lines); RFC 3550 section 6.4.1 (https://www.rfc-editor.org/rfc/rfc3550.txt:
"D(i,j) = (Rj - Ri) - (Sj - Si) = (Rj - Sj) - (Ri - Si)", "J(i) = J(i-1) + (|D(i-1,i)| -
J(i-1))/16"). In the repository: the code each "Implemented in" names, `docs/bench-packet.md`,
`docs/cec.md`, `docs/measurements/host-wakeup-jitter.md`. No GPL source was opened.

## Decision

1. **A level does not detach an autoplay.** `volume`, `volume_step`, `group_volume`,
   `group_volume_step` and `mute` on a room an autoplay holds keep the hold, whoever sends them
   (the TV's remote through the hub, the control page, a front panel); `group`, `ungroup`,
   `join` and `take` still let the room go. Not only the hub's own commands: an autoplay decides
   what the room plays, and a level is not a source change, so a person turning a turntable up
   from the phone is as much still listening to it as one turning the TV down from its remote.
   Exempting only "the hub of the TV input" would make the same keypress mean two things by
   where it came from. The autoplay's end restores the volume and mute the room had before it
   took the room, as for an alarm (`restore`, unchanged).
2. **Alarms are unchanged** (ADR 0076): any person's command naming a ringing alarm's room,
   a volume or a mute included, ends the alarm. An alarm's ramp IS the room's volume, so a
   person setting one takes the room back; an autoplay never sets a volume.
3. **One stereo speaker folds a surround stream.** The Linux client sets `stereo_downmix` for a
   device of exactly two channels with no output map and no two-way when the stream has more
   than two channels (`dsp::stereo_fold`), and the chain then engages from the first frame,
   `sound` or not (six channels must never reach a two-channel device). A device that refuses a
   surround stream's channel count is reopened with two (`device-channels ...
   reason=stream-count-refused`); a device that accepts it (ALSA's plug layer) is fed it as
   before. A stereo or mono stream is byte for byte today's client.
4. **`line_in_source` waits on events, not time.** Its playback ran only the second its steps
   took, while the sync loop asks every 500 ms, takes a reply in at its next tick and discards a
   round trip over 100 ms: one accepted exchange at best unloaded, none under load. The test now
   waits for the published offset before ending the stream. Under heavier load a second cause
   showed: a `source_control` rode the session behind undelivered audio while the role read the
   rest of the script, so the device's failure, not the stop, ended the stream (8 of 8 runs
   failed so). A relay now counts each control into the role's inbox and the test releases the
   next frames only after it landed; the stop's log line, written after `stream_end`, is waited
   for. The code under test is unchanged.
5. **The bench fixture guard cannot lose a match.** `validate-report.sh` and the report half
   piped the 597 fixture hashes (about 39 KB) into `grep -q` under `pipefail`; grep leaving at its
   first match killed the still-writing `printf` with SIGPIPE, the pipeline returned 141, and the
   fixture passed as a hardware capture. In isolation 17 of 200 matches were lost; in the e2e one
   fixture-carrying PR in about fourteen validated, which is the flake (two of three e2e runs
   failed before the fix, three of three passed after). A here-string has no pipe. This was a
   guard hole for real reports too, not only a test flake.
6. **`chorus-udp-loss`** (in `crates/hostprobe`, beside the wakeup probe): one 1472-byte datagram
   per 2.5 ms (the wire's defaults), tallied for loss, the burst-length histogram, reordering,
   duplicates and RFC 3550's transit variation in power-of-two buckets. Monotonic clocks only,
   no clock agreement needed. Built rather than named for a later goal because S8 needs it and
   it is small; one-way delay is not measured (it needs agreeing clocks).
7. **Bench session S8** in `docs/bench-packet.md`: the three TVs' answers first, then the
   capture (with the TV off: stall or zeros), CEC through chorus's own log lines, the ALSA
   controls for the non-audio bit, the TV's ppm (`source-tv ppm=`), the hub's wakeup jitter, the
   lip-sync measurement that sets `av_trim_ms`, the LAN loss histogram, the ARC extractor's CEC
   address, and CEC volume over optical. No topic is added to `tools/bench/topics.conf` (no
   grader); its reports are written by hand with `Source: hardware`.

## Evidence

Nothing here is timing evidence (BRIEF.md section 3.1 rule 3).

- `crates/server/tests/schedule_runtime.rs`: TV keys (three `volume_step`, a `mute`, a
  `group_volume_step`) while the TV plays keep the autoplay and the standby restores 0.400,
  unmuted, in "downstairs"; a `join` still detaches; an alarm still ends on a mute; the line-in
  detach test now detaches by `ungroup` after a volume and a mute did not.
- `crates/server/tests/cec_tv.rs`, the real server: Volume Down twice and Mute from the fake
  TV's remote while the den's TV autoplay plays move the den to 0.520 and mute it, no `detached`
  line, and the standby restores the stream at 0.560, unmuted. Without the change it fails at
  "the den is restored".
- `dsp.rs`: a 5.1 stream of constants into a modelled two-channel device, no `sound`, comes out
  `Lo = FL + 0.7071 FC + 0.7071 BL`, `Ro` likewise, LFE dropped, within 2 LSB.
- `line_in_source`: 20 consecutive passes with the test and eight busy loops pinned to one CPU
  (the unfixed test failed 8 of 8 under the same load).

## Deviations from the follow-ups

- The follow-up asked to exempt "the TV input's own hub's volume/mute commands"; every level
  command is exempt (decision 1, why).
- The `stereo_downmix` follow-up named "a 2-channel device with no output map"; the client
  learns a device is two-channel only when it refuses the stream's count, so the reopen is new.

## ASSUMED values

`chorus-udp-loss` defaults: 2.5 ms interval and 1472 bytes (ADR 0091's own defaults), a summary
line every 600 s; S8's buy lines T2 (DIR9001 module, US$10-20) and T5 (cables, US$10-15), the
HiFiBerry overlay name, the 24 h run length; every TV value, until the Needs item "The three
TVs: model, eARC port, optical out and audio menu" is answered.

## Follow-ups

- A bench script and grader for S8 (a `tv-capture` topic) once a first hand-written report shows
  what to grade.
- `printf | grep -q` on short strings elsewhere in `tools/` is safe (one write before grep can
  exit); a long input piped so would carry the same race.
- The C endpoint sets `stereo_downmix` once it negotiates more than two channels.
