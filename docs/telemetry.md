# Telemetry and the Prometheus exporter

What every speaker reports about itself, what the server keeps of it, and the
text `GET /metrics` answers a Prometheus scrape with (goal 15). The decision
record is `docs/decisions/0000-a-prometheus-exporter-on-the-control-listener.md`;
the wire message is `docs/protocol.md`, "Telemetry".

## The path

1. An endpoint sends a `telemetry` (protocol v2, type 0x15) inside its session
   about once a second: the firmware from `firmware/src/session.c`, the Linux
   endpoint from its playout loop (`crates/client-linux/src/run.rs`,
   `wire_report`).
2. The server keeps each speaker's LATEST report and the instant it arrived
   (`crates/server/src/metrics.rs`, `TelemetryStore`), beside the room model
   and under its own lock. Keeping one changes no state and publishes
   nothing: a report a second per speaker never reaches a control subscriber.
   A report from an id that is not a listed speaker is not kept.
3. `GET /metrics` on the control listener (`--control-listen`) renders what is
   kept, with the speaker list, as Prometheus text exposition format 0.0.4
   (`Content-Type: text/plain; version=0.0.4; charset=utf-8`). It is one more
   route served by the existing control workers: no thread and no port of its
   own, so the server's thread population is what it was. Like every route of
   that listener it has no authentication (`docs/control-plane.md`), and it
   only reads.

The server keeps no history. History is Prometheus's.

## The series

Every per-speaker series carries exactly one label, `speaker`: the id the
speaker's sessions authenticate as (for a board, `chorus-` and twelve hex
digits). The `_info` series carry the labels named below and always have the
value 1.

| Series | Type | Unit and meaning | From |
|---|---|---|---|
| `chorus_speaker_sync_error_seconds` | gauge | signed seconds: the speaker's OWN estimate of its playout error against the server timeline. Not a measured error between speakers | `sync_error_ns` |
| `chorus_speaker_buffer_fill_seconds` | gauge | seconds of audio queued ahead of the speaker's playout point | `buffer_fill_us` |
| `chorus_speaker_rate_correction_ratio` | gauge | signed ratio: the rate correction the speaker's servo has in force (`1e-6` is 1 ppm). A gauge of the correction, not a count of corrections | `correction_ppb / 1e9` |
| `chorus_speaker_resyncs_total` | counter | hard resynchronisations since the speaker's session began | `resyncs` |
| `chorus_speaker_underruns_total` | counter | underruns (events, one per run of underrun silence) since the speaker's session began | `underruns` |
| `chorus_speaker_link_info{speaker,link}` | gauge, 1 | how the speaker says it reaches the network: `link` is `wired`, `wifi` or `unknown` | `link` |
| `chorus_speaker_rssi_dbm` | gauge | Wi-Fi signal strength in dBm (negative; nearer 0 is stronger) | `rssi_dbm` |
| `chorus_speaker_heap_free_bytes` | gauge | free heap right now, bytes | `heap_free_bytes` |
| `chorus_speaker_heap_min_free_bytes` | gauge | the least free heap since the speaker booted, bytes | `heap_min_free_bytes` |
| `chorus_speaker_temperature_celsius` | gauge | degrees Celsius | `temperature_centi_c / 100` |
| `chorus_speaker_firmware_info{speaker,version}` | gauge, 1 | the version the speaker last said it runs: its `firmware_status` version when it has an update unit, else its `hello`'s software string | the room model |
| `chorus_speaker_connected` | gauge | 1 while a session of the speaker is up, else 0; present for EVERY adopted speaker | the room model |
| `chorus_speaker_info{speaker,name,room}` | gauge, 1 | the display name and the assigned room (`room=""` when it has none) | the room model |
| `chorus_speaker_telemetry_age_seconds` | gauge | seconds since the latest report arrived, on the server's monotonic clock | the store |
| `chorus_server_build_info{version}` | gauge, 1 | the chorus-server build that answered | the build |
| `chorus_speakers` | gauge | adopted speakers, connected or not | the room model |

The two counters restart at 0 when a speaker reconnects (they are "since the
session began" on the wire); `rate()` and `increase()` treat that as the
counter reset it is.

## The rules of the text

- **Unknown is omitted, never zero.** A value an endpoint reports as unknown
  (the wire's sentinels: `i64` minimum for the sync error, -128 for the RSSI,
  `i16` minimum for the temperature, `u32` maximum or no heap block for the
  heap) leaves that speaker's sample out. A zero would be a measurement: a
  perfect clock, 0 dBm, 0 degrees. `chorus_speaker_rssi_dbm` is also omitted
  for a speaker whose link is `wired`, whatever it sent.
- **A disconnected speaker keeps three series:** `chorus_speaker_connected`
  (0), `chorus_speaker_info` and `chorus_speaker_firmware_info` (the last
  version it said; absent if it never said one since the server started).
  Everything else of it leaves the scrape with its session, and its kept
  report is dropped, so Prometheus marks those series stale at the next scrape
  instead of drawing the last value on.
- A connected speaker that has sent no report yet (the first second of a
  session, or an endpoint built before it sent any) has the same three series
  with `connected` 1.
- Every family has a `# HELP` and a `# TYPE` line before its samples and is
  one group; a family with no sample at all is left out whole. No timestamps:
  the scrape time is the sample time.
- Label values escape backslash, double quote and line feed, and nothing
  else. A speaker's name is whatever a person typed.
- Numbers are the wire's integers with the decimal point moved, so no float
  formatting is involved (`-1250` ns is `-0.000001250`).

## Which endpoint fills what

`known` means the endpoint has a source for the value and sends it; `unknown`
means it sends the wire's "unknown" and the series is omitted.

| Field | ESP32-S3 firmware (a board) | C endpoint on a host (the gate's) | Linux endpoint |
|---|---|---|---|
| sync error | known once the playout loop has formed an error (`chorus_playout_stats`, `last_error_ns`); unknown before | the same code | known once the playout loop has formed an error against the device's reported delay; unknown before, and on a device that reports no delay |
| buffer fill | known: frames queued in the jitter buffer, as time at the stream's rate. Not the I2S FIFO after the writer | the same code | known: frames queued in the client's buffer. Not the sound device's own delay |
| rate correction | known: the servo's correction in force | the same code | known: the correction in force |
| resyncs | known: hard resyncs since the session began | the same code | known: hard resyncs of this session |
| underruns | known: underrun events since the session began | the same code | known: underruns of this session (the device's and the writer's) |
| link | known: the board profile's transport (`wired`, `wifi`); `unknown` on the emulator's board, whose link is neither | `unknown`, unless stated on the command line | known: what the endpoint was TOLD with `--transport` (default wired). A declaration, not a probe |
| RSSI | known on a wireless board that is associated (`esp_wifi_sta_get_rssi`); unknown otherwise | unknown, unless stated | **unknown**: nothing reads the host's radio |
| heap | known: the internal 8-bit heap, free and minimum free (`heap_caps`), the figures the console's `resources` prints. PSRAM is not in it | unknown, unless stated | **unknown**: a process on a general-purpose OS has no figure that means what a microcontroller's free heap means |
| temperature | **unknown**: no board profile has a temperature sensor | unknown, unless stated | **unknown** |
| firmware version | the image's version, from `firmware_status` | `chorus-endpoint <version>`, from `hello` | `chorus-client <version>`, from `hello` |

Without a playout path (the plain host session program, which counts chunks
and plays nothing) the four playout figures are sent as 0, because the wire
has no "unknown" for them. No board and no shipped endpoint is in that case.

The host C endpoint's "unless stated" is the test seam: the session asks a
`health` callback for link, RSSI, temperature and heap
(`firmware/include/chorus/session.h`); the board binds it to the platform
(`firmware/main/esp_hal.c`), and the host program
`chorus-endpoint-dsp-session` binds it to five `--health-*` options. Those
are stated fakes, a test's numbers, never a measurement.

## What is ASSUMED, and what is not claimed

- **ASSUMED: one report a second** (`CHORUS_SESSION_TELEMETRY_INTERVAL_NS`,
  `REPORT_INTERVAL_US`). The protocol says "about once a second is the
  intent"; no measurement chose it.
- **The sync error is not timing evidence.** It is each speaker's own view of
  its own playout loop. Whether two speakers are in time is measured at their
  outputs (BRIEF.md section 3.1 rule 3, `docs/measurements/`), never read off
  this gauge, and a rule or a dashboard that shows it should say so.
- **The board's values are not host-gradable.** The ESP-IDF binding in
  `esp_hal.c` compiles in the gate's firmware images and runs under the
  emulator, but no test in this repository has read an RSSI or a heap figure
  off a real board.
- **A temperature has no source.** The ESP32-S3's die sensor exists but is the
  SoC's temperature, not the board's or the amplifier's, and reading it needs
  a driver the image does not carry today. Wiring it (or a board sensor) is a
  named follow-up, and until then the series does not exist for a real
  speaker. Nothing is invented in its place.
- **RSSI and heap on a Linux endpoint** are named follow-ups in the same
  sense: unknown today, by decision, not by oversight.

## Checking it

- `cargo test -p chorus-server --test metrics_scrape -- --nocapture` is the
  line-A test: the real server, the real C endpoint (real playout figures,
  stated fakes through the health seam) and a Linux-client session, ONE
  scrape, every series above asserted, and the scrape printed. It runs in
  `make gate`.
- `make verify-metrics` lints such a scrape with `promtool check metrics`
  (Prometheus 3.13.3's promtool, pinned in `mise.toml`). Where promtool is
  absent or another version it refuses by name and exits non-zero; it is never
  reported as passed.
- The format's rules (HELP and TYPE, one group per family, escaping, omission)
  are unit tests of `crates/server/src/metrics.rs` and run in the gate with no
  tool.
- The wire: `fixtures/protocol/v2/telemetry.hex` (no heap block, the 36 bytes
  of before) and `telemetry_heap.hex` (with it), read by both the Rust and the
  C codec.

## Retention, and where evidence lives

The homelab's Prometheus keeps 7 days. That is enough for a dashboard and for
alerting, and it is not an archive: a graph is gone after a week. Anything
that is evidence (a soak, a sync measurement, a bench session) is written up
as a report under `docs/measurements/` with its provenance lines
(`docs/conventions.md` rule 11), with the numbers in the report itself and
not a link to a dashboard.

## Bench

What the gate cannot show and a bench session with real boards would, as one
item in the owner's queue: flash a wired and a Wi-Fi speaker with a build that
has this exporter's firmware half, scrape `/metrics`, and record (in a report
under `docs/measurements/`) that the wired speaker shows `link="wired"`, a
heap figure and no RSSI; that the Wi-Fi speaker shows `link="wifi"` and an
RSSI that moves the right way when the speaker is carried away from the
access point; that `heap_min_free_bytes` stays above a floor through an hour
of playback (the floor the alert rule then uses, replacing an ASSUMED one);
and that pulling a speaker's power leaves exactly `connected` 0, `info` and
`firmware_info` of it in the next scrape. Flashing is the owner's action.
