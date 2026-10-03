# 0000: the server keeps each speaker's latest telemetry beside the room model and serves it as Prometheus text on the control listener's `GET /metrics`; heap rides on `telemetry` as an optional trailing block; both endpoints fill what they know and say unknown for the rest

- Status: accepted (goal 15, 2026-10-03)
- Decided by: the goal (program line A; BRIEF.md section 5.9's "per-client telemetry ...
  aggregated at the server and exposed for Prometheus/Grafana") inside the coordinator's
  goal-15 design envelope (section 1, which fixes the series names), track
  `chorus-g15/exporter`; every default below not cited is ASSUMED
- Implemented in: `crates/server/src/metrics.rs` (new: the store and the text),
  `control.rs` (the store's field, `telemetry_reported`, `metrics`, the route, the forget at
  a speaker's last session's end), `session.rs` (the handler's `telemetry` arm);
  `crates/protocol/src/v2/` (`Telemetry`'s two heap fields, the codec, the two constants);
  `firmware/src/protocol_v2.c`, `session.c` (the fill, the `health` seam, the baselines),
  `playout.c` (underruns as events), `firmware/main/esp_hal.c` (the board's binding),
  `firmware/tests/main_dsp_session.c` (the stated fakes); `crates/client-linux/src/run.rs`
  (`wire_report`, the send); `fixtures/protocol/v2/telemetry_heap.{fields,hex}`;
  `crates/server/tests/metrics_scrape.rs` (the line-A test); `tools/verify-metrics.sh`,
  `tools/lib.sh` (`require_promtool`), `mise.toml`. The contracts are `docs/telemetry.md`
  and `docs/protocol.md` ("Telemetry")

## Context

Protocol v2 has carried a `telemetry` message (0x15) since goal 5, and nothing used it: the
server counted the messages and discarded the payload, the firmware filled two of its nine
fields, the Linux endpoint sent none, and heap, which the brief lists, was not on the wire at
all. Goal 15 wants ten per-speaker metrics scraped by the homelab's Prometheus: sync error,
buffer fill, corrections, resyncs, underruns, link, RSSI, heap, temperature and firmware
version. The server's thread population is fixed and graded (`6 + 2N + M`), a control state
is fanned out to every subscriber on every change, and the two codecs (Rust and C) must not
drift.

## What was read

All read 2026-10-03: `BRIEF.md` sections 3.1, 3.2 and 5.9; the goal-15 design envelope
(section 1 and "Rules every track follows") and its code survey (section B); the goal's
research note on the exposition format (its section 4), which quotes and cites, each read
2026-10-03 by that note: the Prometheus exposition formats page
(https://prometheus.io/docs/instrumenting/exposition_formats/), the naming practices
(https://prometheus.io/docs/practices/naming/), writing exporters
(https://prometheus.io/docs/instrumenting/writing_exporters/), the staleness section of
querying basics (https://prometheus.io/docs/prometheus/latest/querying/basics/), the promtool
page (https://prometheus.io/docs/prometheus/latest/command-line/promtool/), and
node_exporter's wifi collector (Apache-2.0) for the `_dbm` precedent. `docs/protocol.md`
("Versions", "0x11 capabilities", "Telemetry", "Decoder behaviour", "Where the skip rule
stops applying"), `docs/control-plane.md`, `docs/conventions.md` (rules 9, 10, 13, 14, 15),
ADRs 0108, 0110 and 0112; the code each "Implemented in" names and
`crates/control/src/speakers.rs`, `firmware/include/chorus/playout.h`, `telemetry.h`,
`firmware/main/console_esp.c`, `crates/server/tests/adoption.rs` and `common/mod.rs`.
ESP-IDF v6.1 (Apache-2.0) at the pinned tag: `components/esp_wifi/include/esp_wifi.h`
(`esp_wifi_sta_get_rssi`). The Prometheus release page for 3.13.3
(https://github.com/prometheus/prometheus/releases/tag/v3.13.3; the linux-amd64 archive's
sha256 as `mise.lock` carries it). No GPL source and no reciprocally licensed design file
was opened.

## Decision

1. **The exporter is a route, not a service.** `GET /metrics` is one more arm of the control
   listener's router, answered by an existing control worker with
   `Content-Type: text/plain; version=0.0.4; charset=utf-8`. No thread, no port, no flag: the
   thread population and its graded formula are unchanged, and a server without
   `--control-listen` has no exporter, as it has no state route. Not chosen: a listener of
   its own (`--metrics-listen`), which needs an acceptor and a worker in the fixed
   population for one request a minute; and a push gateway, which the homelab does not run.
   The listener has no authentication, as before; the route only reads.

2. **The latest report per speaker is kept beside the room model, never in it.**
   `TelemetryStore` is a map from speaker id to the last `telemetry` and the `Instant` it
   arrived, under its own lock, held by `ControlState`. The session's handler hands a report
   to `telemetry_reported`, which keeps it only for an id that is a listed speaker (so the
   store is bounded as the speaker list is, at 64). Nothing is persisted, no serial moves and
   no state is published: routing a report a second per speaker through `speaker_now` would
   have fanned a whole state out to every subscriber every second. When a speaker's last
   session ends its kept report is dropped.

3. **The series are the envelope's, and unknown is omitted.** The names, types and units are
   those of the design envelope's table (`docs/telemetry.md` repeats it with each series'
   source); base units (seconds, bytes, celsius, a ratio), `_total` on the two counters,
   `_info` series with the value 1 for the link and the firmware version, one `speaker` label.
   A value the endpoint reported as unknown leaves the sample out; a disconnected speaker
   keeps `chorus_speaker_connected` 0, `chorus_speaker_info` and
   `chorus_speaker_firmware_info` and nothing else, so its measurements go stale at once
   rather than being drawn on. A family with no sample is left out whole. Numbers are written
   from the wire's integers by moving the decimal point: no float formatting, so nothing
   about a value depends on the scrape. `chorus_speaker_firmware_info` for a speaker that has
   said no version since the server started is omitted rather than given an empty label.
   "Corrections" is the rate correction in force, a gauge: BRIEF.md section 5.9 lists it as
   the "correction rate", and it is what the wire carries.

4. **Heap rides on `telemetry` as an optional trailing block.** Two `u32`s after the 36
   bytes, free heap and minimum free heap, the `u32` maximum for a figure not known, written
   only when one of them is known. This is the protocol's own rule for adding a field ("a
   payload longer than the fields a decoder knows about is accepted and the excess ignored")
   and the precedent of `capabilities`' `features` byte: the existing `telemetry` vector is
   byte for byte unchanged, every existing fixture is untouched, and a server built before
   the block ignores it. A new vector, `telemetry_heap`, is read by both codecs. Not chosen:
   a new message type (a second message a second per speaker, a second arrival time for half
   of one snapshot, and a type byte and a decoder in two languages for eight bytes); and a
   `capabilities` feature bit, which announces something a server may then send an endpoint,
   where this is only something an endpoint says and a server may ignore. A block is read
   whole or not at all, so a later, longer block still reads here as this one.

5. **An endpoint fills what it has a source for, through seams.** The firmware's session
   takes buffer fill, underruns, resyncs and the rate correction from the playout path it
   already reads the sync error from, and asks a new optional `health` callback on the
   session's configuration for link, RSSI, temperature and heap, handing it a record in which
   everything is unknown. The board binds it in `esp_hal.c` (the profile's transport,
   `esp_wifi_sta_get_rssi` on a wireless board, `heap_caps` over the internal heap); the host
   DSP session program binds it to five command-line options, which are stated fakes for
   tests. Temperature has no source on any board profile and stays unknown. The playout path
   gains a count of underrun EVENTS (it counted frames), because that is what the wire field
   and a `_total` counter mean, and the session reports both counters since the session
   began, as the wire says. The Linux endpoint sends the same message once a second on the
   writer its time-sync exchange already uses: its error, buffer, underruns, hard resyncs,
   correction and the link it was told; RSSI, temperature and heap stay unknown there.

6. **promtool lints a real scrape, outside the gate.** `make verify-metrics` runs the line-A
   test, takes the scrape it wrote, and hands it to `promtool check metrics`. promtool is the
   one in the Prometheus 3.13.3 release, pinned in `mise.toml` with its sha256 in `mise.lock`
   like every other tool. It is not a gate step: the format's rules are held in the gate by
   the server's own unit tests and the line-A test, with no tool; where promtool is absent or
   another version the target refuses by name and exits non-zero, and
   `tools/unrun-checks-are-visibly-unrun.sh` holds it to that. Why a pinned linter at all
   when the tests assert the format: promtool is the reference parser's own, so it catches a
   reading of the format that chorus's tests and chorus's renderer would share.

## The line-A test

`the_exporter_serves_every_listed_metric_in_a_test_scrape` (`crates/server/tests/metrics_scrape.rs`)
starts the real `chorus-server`, the real C endpoint (`chorus-endpoint-dsp-session`: the
firmware's session, playout path and telemetry sender on a host) and a Linux-client session
that sends the report the shipped client builds. One scrape must hold every series of the
table for the C speaker with plausible values, the Linux speaker's known series with exactly
the values sent and none of the four it does not know, HELP and TYPE for exactly the sixteen
documented families; then the C endpoint is stopped and exactly three series of it remain.
The C speaker's playout figures are real; its link, RSSI, temperature and heap are the stated
fakes, and the test's documentation says so. It prints the scrape.

## No third-party crate

The exposition is about three hundred lines of string building over integers the server
already holds, with the escaping rule stated in one sentence of the format. A metrics crate
would bring a registry, atomics and float formatting for a server that has one scrape a
minute and no histogram (BRIEF.md section 3.2: build what is small and instructive).

## ASSUMED

One report a second (the protocol's "about once a second is the intent"); the store's bound
is the speaker list's (64, ADR 0106); the scrape's plausibility bands in the line-A test
(a sync error under 1 s, a buffer under 5 s, a correction under 1e-2) are sanity bounds on a
loaded host and not budgets. Adding Prometheus to `mise.toml` makes `mise install` fetch its
release archive (about 120 MB) on a machine that has none.

## Follow-ups

- The bench item (`docs/telemetry.md`, "Bench"): real boards' link, RSSI and heap in a
  scrape, and the heap floor an alert rule can cite. The owner's action; the coordinator
  files it.
- A temperature source: the SoC's die sensor or a board sensor, with what it means said in
  the series' HELP. Until then no real speaker has the series.
- RSSI and a memory figure on the Linux endpoint, if one is ever wanted there.
- The control state's `speakers[].link` comes from `attach`, the exporter's
  `chorus_speaker_link_info` from `telemetry`: two reports of one fact, which may disagree
  for an endpoint told the wrong `--transport`. Left as it is.
