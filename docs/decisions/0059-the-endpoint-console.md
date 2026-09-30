# 0059: the endpoint has a serial console for the bench, with runtime-only values, and chorus-measure recovers the produced sample rate

- Status: accepted (goal 8, 2026-09-30)
- Decided by: the goal (audit A-13 and A-11, assigned to goal 8 by goal 2; goal 7's deferred S3
  decode-cost measurement)
- Implemented in: `firmware/src/console.c`, `firmware/src/decode_cost.c` (and their headers),
  `firmware/main/console_esp.c`, the session's two hooks (`firmware/include/chorus/session.h`,
  `server_update` and `on_telemetry`), `crates/measure/src/rate.rs` (`chorus-measure rate`),
  `tools/lib.sh` (`endpoint_console`, `require_endpoint_console`),
  `tools/wireless-characterization-run.sh`, `tools/endpoint-rig-run.sh`,
  `tools/decode-cost-run.sh`, `tools/bench/topics.conf`; held by `firmware/tests/test_console.c`
  (`make firmware-check`), `crates/measure/tests/produced_rate.rs` and `tools/bench/e2e-test.sh`

## Context

Audit A-13: the wireless characterization wrote `power-save <mode>` and `server <host:port>` to a
serial port on which the firmware read nothing, so a report could name a power-save mode the radio
was never in. Audit A-11: the endpoint rig's AC-3 (the produced sample rate, measured rather than
read back) handed a WAV to a command that reads offset series; goal 7 made the report say "not
analysed" instead. Goal 6 measured the decoders' cost on the host only; goal 7 moved the S3's own
figure to the console goal 8 builds.

## What was read

All read 2026-09-30: `docs/audit/2026-09-audit.md` (A-5, A-11, A-13), the two scripts,
`tools/lib.sh`, `tools/bench/*`, `crates/measure/src/{freerun,lag,chirp,wav,config}.rs`,
`firmware/src/{session,wifi,telemetry,codec}.c`, `firmware/tests/test_codec.c`, and from the pinned
ESP-IDF v6.1 tree (Apache-2.0, commit `fff9895c82d744c7237be8847347bdd1b07c6643`):
`components/console/esp_console.h` (the REPL, `esp_console_new_repl_uart`,
`esp_console_new_repl_usb_serial_jtag`, `ESP_CONSOLE_REPL_CONFIG_DEFAULT` with its 4096-byte task
stack), `components/esp_stdio/include/esp_stdio_cli_config.h` and `components/esp_stdio/Kconfig`
(the console device choice), `examples/ethernet/iperf/main/ethernet_iperf_main.c` (a REPL set up
the same way), `components/esp_wifi/include/esp_wifi.h` (`esp_wifi_set_ps`; default
`WIFI_PS_MIN_MODEM`). Online: the ESP-IDF console guide,
https://docs.espressif.com/projects/esp-idf/en/v6.1/esp32s3/api-reference/system/console.html,
read 2026-09-30. No GPL source.

## Decision

1. **The console's decisions are a pure unit, graded on a host.** `console.c` parses a line and
   writes one reply line, `<command> key=value ...` or `error <command> reason=<token> detail="..."`,
   so a script reads key=value words and nothing else. Commands: `power-save <mode>` (sets the mode
   through the same radio interface the bring-up uses and replies with the platform's own readback;
   a readback that disagrees is an error reply carrying both words, and a wired endpoint refuses
   by name without touching a radio), `server <host:port>` (checked here, applied at the session's
   next connection attempt), `status` (the telemetry line the session last published), and
   `decode-cost [fixture]`. The REPL starts as soon as the committed configuration has parsed,
   before the amplifier and the link, so it answers on a board whose bring-up stops early (bench
   packet S6 relies on that). The ESP-IDF binding only joins argv back into a line and prints the
   reply; it registers chorus's own `help`, which is how the scripts recognise a chorus console.
2. **Values are runtime only.** Nothing the console sets is written to flash or NVS; a reboot
   returns the endpoint to its committed configuration. Provisioning into NVS is goal 14's
   (adoption and Wi-Fi provisioning), and a console that wrote NVS would be a second provisioning
   path nobody reviewed.
3. **The session takes two optional hooks** rather than a console dependency: `server_update`,
   asked before every connection attempt (a new address that does not split is refused in the event
   log and the old one kept), and `on_telemetry`, handed the telemetry at every event it publishes.
   NULL keeps the session exactly as it was, so every existing caller and test is unchanged. The
   binding holds the shared state under one FreeRTOS mutex (the session and the REPL are different
   tasks).
4. **decode-cost decodes committed fixtures through the endpoint's own codec seam** and times only
   the decode calls on the monotonic clock it is handed; a figure is published only for a decode
   that hashed to the fixture's `decode_fnv1a64`. The image embeds `flac-s16-stereo-44k1` and
   `opus-tv10-celt-stereo` (about 125 KB of flash; the image went from 1,062,864 to 1,234,064
   bytes with the console component, 20% of the 1.5 MB app partition still free). The reply carries
   the console task's least free stack (`stack_free_bytes`) because the decoders run on that task,
   whose 16 KB stack is ASSUMED until a bench run reports it. `tools/decode-cost-run.sh` is the
   bench entry point (topic `embedded5-decode-cost`, result MEASURED, taken with the stream
   stopped).
5. **The scripts drive the console and trust only its replies.** `endpoint_console` (in
   `tools/lib.sh`) sends a line and returns the reply, skipping the REPL's echo and prompt;
   `require_endpoint_console` asks `help` and refuses by name when the reply is not chorus's. The
   wireless run keeps every `power-save` readback in its raw data, reports it per mode, and stops
   on a readback that disagrees; both rig scripts stop when `server` is not accepted.
6. **`chorus-measure rate <capture.wav>` recovers the produced rate** (A-11): every
   `rate_hop_us` it finds where the next chirp sweep starts (a one-period reference correlated
   against the capture, refined by a parabola), unwraps the starts into whole periods, and fits the
   residual against elapsed time with the free-run fit, whose refusals (too short, too noisy) it
   inherits. The figure is against the capture interface's clock and says so. It prints and writes
   nothing, so it cannot overwrite a baseline (A-5). Thresholds `rate_hop_us`, `rate_min_span_s`
   and `rate_max_half_width_ppm` are in `config/measure.conf`. On the committed fixture
   `18-rate-skewed` (channel A 150 ppm fast, channel B on the clock) it reads +149.586 and +0.004
   ppm.
7. **The endpoint rig grades AC-3 against the servo's correction authority.** AC-3 names no ppm
   bound; the report holds each resolved figure to `max_correction_ppm` in `config/sync.conf`,
   because a rate error the servo cannot correct is one no endpoint may have. A figure outside it
   is a FAIL; no resolved figure leaves the run INCOMPLETE.

## Not chosen

- **esp_console's argtable parsing per command:** it would put the refusal rules in the binding,
  which is not host-graded. One line parser in `console.c` keeps every rule under test.
- **Writing console values to NVS:** see 2; goal 14 owns persistence.
- **A decode-cost task of its own:** the REPL task already exists; a second 16 KB internal-RAM
  stack beside it costs DIRAM the playout path needs (goal 6's note: 88 % used).
- **Rate from the capture's zero crossings or an FFT peak:** the chirp is what the rig already
  plays and what the lag analyser already correlates against; its sweep starts are sharp, and
  reusing the free-run fit reuses its refusals.
