# 0000: a speaker's diagnostics are eight Home Assistant sensors read from `GET /metrics` by a second coordinator, one scrape a minute and only while one of them is enabled, all in the diagnostic category and all but the link and the firmware version disabled by default

- Status: accepted, 2026-10-04. Extends 0138 (the Home Assistant integration) and 0154 (the
  speaker devices); uses 0115 (the Prometheus exporter on the control listener) as it is.
  It replaces three of 0138's quality-scale verdicts: `appropriate-polling` and
  `entity-disabled-by-default` are `done` and no longer `exempt`, and `entity-category` now
  covers diagnostic entities too.
- Decided by: the owner for what there is (K83: per-speaker sync error, buffer fill,
  corrections, resyncs, link, RSSI, temperature and firmware version as diagnostics; I13 and
  the goal's "rate-limited, mostly disabled by default"); this record for the rate, the set
  that starts enabled, the units, and what a failed scrape does.
- Recorded by: the owner's agent harness, in the pull request that adds this record.
- Implemented in: `integrations/homeassistant/custom_components/chorus/` (`sensor.py`,
  `coordinator.py`, `const.py`, `_aiochorus/metrics.py`, `_aiochorus/client.py`,
  `strings.json`, `icons.json`, `quality_scale.yaml`),
  `integrations/homeassistant/tests/test_sensor.py`, `tests/aiochorus/test_metrics.py`,
  `tests/fake_server.py`, `tests/fixtures/metrics-scrape.txt`,
  `integrations/homeassistant/README.md`, `docs/home-assistant.md`

## Context

A speaker reports about itself once a second, and the server keeps the latest report and
renders it on `GET /metrics` (`docs/telemetry.md`). The state message deliberately carries
none of it: a report a second per speaker never reaches a control subscriber
(`docs/control-plane.md`, `docs/telemetry.md`, "The path"). So the only place an HTTP client
can read a speaker's telemetry is the exporter's text, and reading it is a poll. No server
change was needed and none is made.

## What was read

All on 2026-10-04. `docs/telemetry.md` ("The series", "The rules of the text", "Which endpoint
fills what", "What is ASSUMED"), the exporter's source `crates/server/src/metrics.rs` (the
families, their HELP and TYPE text, the number formatting), the line-A test
`crates/server/tests/metrics_scrape.rs`, and 0138's exemptions. Home Assistant core 2026.9.3
(Apache-2.0): the `sensor` platform's device and state classes, `DataUpdateCoordinator`, and
the quality-scale rules `appropriate-polling`, `entity-category` and
`entity-disabled-by-default`.

## Decision

**The exporter carries every K83 diagnostic, two of them with a caveat.** The shaper's
assumption was checked first, against `docs/telemetry.md` and `metrics.rs`:

| K83 diagnostic | Series | Sensor | Caveat |
|---|---|---|---|
| sync error | `chorus_speaker_sync_error_seconds` | Sync error, microseconds | the speaker's OWN estimate, not a measured error between speakers and not timing evidence |
| buffer fill | `chorus_speaker_buffer_fill_seconds` | Buffer fill, milliseconds | |
| corrections | `chorus_speaker_rate_correction_ratio` | Rate correction, ppm | the correction the servo has in force, a gauge. The exporter has no COUNT of corrections, and none is invented |
| resyncs | `chorus_speaker_resyncs_total` | Resyncs | since the session began; 0 again on a reconnect |
| link | `chorus_speaker_link_info{link}` | Link: wired, Wi-Fi or unknown | |
| RSSI | `chorus_speaker_rssi_dbm` | Signal strength, dBm | omitted for a wired speaker and for an endpoint with no radio to read |
| temperature | `chorus_speaker_temperature_celsius` | Temperature, °C | the series exists, and **no real endpoint fills it**: no board profile has a temperature sensor (`docs/telemetry.md`, "What is ASSUMED"). The sensor exists and reads unknown until a speaker reports one |
| firmware version | `chorus_speaker_firmware_info{version}` | Firmware version | kept while the speaker is disconnected |

So nothing K83 names is missing from the exporter as a series. What is missing is behind the
series, and each is a follow-up that is the server's or the firmware's, not this
integration's: a count of corrections (a new series), and a temperature source on a board
(`docs/telemetry.md` already names it). The exporter's underruns and heap figures are not in
K83's list and are not sensors.

**One scrape a minute, and none with nothing enabled.** A second coordinator
(`ChorusMetricsCoordinator`) polls `GET /metrics` every 60 s. One scrape serves every speaker.
A coordinator polls only while it has a listener and a disabled entity is never added, so
with no diagnostic sensor enabled the server is not asked at all. The first enabled sensor
added starts one scrape at once, so a sensor has its value when the entry has loaded and not
a minute later. A refresh asked for less than 55 s after the last scrape (the
`homeassistant.update_entity` action) is answered from that scrape, on a monotonic clock, so
within one loading of the entry no two scrapes are less than 55 s apart. The floor is 55 s
and not 60 s so that a scheduled refresh that lands a little before the full minute is
never the one skipped. Reloading the entry starts again with one scrape.

60 s is a choice, not a measurement: the values a person watches here (a link, a version, a
signal strength, a drift) move over minutes, a dashboard that wants a second's resolution is
Prometheus's (`docs/telemetry.md`), and each enabled numeric sensor is one recorder row per
change.

**Diagnostic, and mostly disabled.** All eight have the diagnostic entity category. Enabled
by default: **Link** and **Firmware version**, which change almost never, so an idle install
writes almost nothing. Disabled by default: Sync error, Buffer fill, Rate correction,
Resyncs, Signal strength and Temperature, each a number that moves at every scrape. They are
created for every adopted speaker from the state's `speakers[]`, with no scrape needed, and
leave with the speaker's device.

**Unavailable is a failed scrape or an ended session; unknown is a value the speaker does not
have.** A scrape that fails (no answer, a status that is not 200) or is not the exporter's
text (no `chorus_speakers` sample, a line that is not a sample, a value that is not a finite
number, bytes that are not UTF-8) makes every diagnostic sensor unavailable and is logged
once. The state's coordinator, the event stream and every other entity do not depend on the
scrape. A speaker whose session ended keeps its firmware version in the scrape and nothing
else, so its other sensors are unavailable. A connected speaker's omitted series (the
exporter's "unknown is omitted, never zero") is the state `unknown`, never 0.

**The reader is in the vendored client and is not a Prometheus parser.** `Metrics.parse`
reads the exporter's own rules: one sample a line, the three label escapes, no timestamps.
Families it does not use are skipped.

**State classes.** `measurement` for the gauges, `total_increasing` for Resyncs (a counter
that restarts with the session), none for the link (an enum) and the version (text).

## Consequences

- `quality_scale.yaml`: 47 done, 7 exempt (0152 said 45 and 9). `appropriate-polling` is
  `done` with the rate in its comment; the manifest stays `local_push`, because the state is
  pushed and only these sensors are polled.
- The tests' sample scrape (`tests/fixtures/metrics-scrape.txt`) is written in the exporter's
  format and is not a capture of a running server. `tests/aiochorus/test_metrics.py` holds
  its families, order, HELP and TYPE lines to `crates/server/src/metrics.rs`, so a change to
  the exporter's text fails there. A scrape of the real server through the integration is
  not part of the live-server test yet: a named follow-up.
- A count of corrections and a real temperature are follow-ups outside this integration, as
  above. Until a board has a temperature source the Temperature sensor reads unknown.
- Nothing here is timing evidence. The Sync error sensor is each speaker's own view; whether
  two speakers are in time is measured at their outputs (BRIEF.md section 3.1).
