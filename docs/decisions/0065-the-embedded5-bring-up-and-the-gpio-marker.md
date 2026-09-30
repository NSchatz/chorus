# 0065: the endpoint reports its heap, stacks and FIFO on its console, marks server-timeline boundaries on a GPIO, and EMBEDDED-5's bring-up is a bench run with disconnect abuse

- Status: accepted (goal 9, 2026-09-30)
- Decided by: the goal (brief section 13 item 4, the EMBEDDED-5 packet; the goal-8 ledger's
  hand-over: free heap, stack high-water marks, amp and FIFO latency after the DMA, the GPIO
  marker)
- Implemented in: `firmware/src/console.c` (`resources`), `firmware/src/playout.c`
  (`chorus_playout_fifo`, `chorus_playout_marker_due`, the marker's planning in the writer),
  `firmware/main/esp_marker.c`, `firmware/main/console_esp.c`, `firmware/main/esp_playout.c`,
  `firmware/main/esp_hal.c` (the interrupt's one call), `pin_marker` and `marker_period_ms` in
  `firmware/config/endpoint.conf` and the board profile, `tools/embedded5-bringup-run.sh`,
  `tools/bench/topics.conf` (`embedded5-bringup`), `docs/bench-packet.md` session S7; held by
  `firmware/tests/test_playout.c`, `firmware/tests/test_console.c`, `firmware/tests/test_i2s.c`
  (`make firmware-check`) and `tools/bench/e2e-test.sh`

## Context

Goal 8 built the playout path (ADR 0058) and the console (ADR 0060) and left to this packet what
only the board can say: how much heap and stack the image really has, how many frames sit between
the writer and the pins, and the GPIO marker BRIEF.md section 10 asks for ("a marker pattern in the
stream toggles a GPIO on each endpoint at the moment of I2S write; logic analyzer measures the
edge delta, isolating the servo from analog path differences"). BRIEF.md section 8 item 5 asks that
the embedded endpoint "survives disconnect/reconnect abuse unattended".

## What was read

All read 2026-09-30. From the pinned ESP-IDF v6.1 tree (Apache-2.0, commit
`fff9895c82d744c7237be8847347bdd1b07c6643`):
`components/esp_driver_gptimer/include/driver/gptimer.h` (`gptimer_start`, `gptimer_stop`,
`gptimer_set_raw_count` and `gptimer_set_alarm_action` carry "This function is allowed to run
within ISR context"; callbacks run in ISR context), `components/heap/include/esp_heap_caps.h`
(`heap_caps_get_free_size`, `heap_caps_get_minimum_free_size`),
`components/freertos/FreeRTOS-Kernel/include/freertos/task.h` (`uxTaskGetStackHighWaterMark`,
`xTaskGetHandle`) and `components/freertos/config/include/freertos/FreeRTOSConfig.h:216`
(`INCLUDE_xTaskGetHandle 1`), `components/console/esp_console_common.c:330` (the REPL task
"console_repl"), `components/lwip/port/include/lwipopts.h:859` (`TCPIP_THREAD_NAME "tcpip"`); the
managed component espressif/w5500 2.0.0 `src/esp_eth_mac_w5500.c:239` ("w5500_tsk"). The Audio
Brick's pin tables: sonocotta/esparagus-media-center (Apache-2.0) `README.md`, "Board Pinout",
https://raw.githubusercontent.com/sonocotta/esparagus-media-center/HEAD/README.md. No GPL source.

## Decision

1. **`resources` on the console.** One line: internal heap free now and least since boot, the
   same for PSRAM (`none` without it), the least free stack of each named task (`chorus-playout`,
   `chorus-session` (network and the inline FLAC/Opus decode), `chorus-amp-fault`,
   `console_repl`, `tcpip`, `w5500_tsk`, `wifi`; `absent` when not running), the FIFO after the
   writer and the marker's counts. A value the image cannot give prints `none`, `absent` or
   `unknown`, never a zero that looks measured. Pure and host-graded; the binding only reads
   ESP-IDF's figures. The "wifi" task name is ASSUMED (the driver is a binary blob).
2. **The FIFO after the writer is the loop's own device delay** (`chorus_playout_fifo`: written
   minus DMA-consumed, less what of the buffer in flight has played since the interrupt's stamp),
   so the console reports exactly the figure the servo uses. The amplifier's own latency after the
   pins is NOT in it: that is a datasheet figure (ADR 0064's `docs/research/tas5825m-register-map.md` states none) plus what the
   rig measures, and nothing here invents it.
3. **The marker marks server-timeline boundaries, not a pattern in the audio.** Every
   `marker_period_ms` (1000) on the server timeline is a boundary; the writer arms the first frame
   whose timestamp is at or after it, with how far past the boundary that frame is. The I2S
   `on_sent` interrupt, whose model is the device delay's (a consumed frame is at the pins), sees
   the buffer holding that frame start and computes the delay to the boundary itself in 16.16
   fixed point (no division, no float); a GPTimer one-shot at 1 MHz then sets the pin to the
   boundary's index modulo 2, so two endpoints of a pair drive the same level at the same server
   instant and a logic analyzer reads their edge delta directly. No stream change and no server
   change: the server timeline is already shared. A boundary whose buffer started unseen, or whose
   frame moved because the DMA ran dry, is counted missed and never fired late.
4. **`pin_marker = none` on the reference board, ASSUMED.** The maker's README names no free
   broken-out GPIO on the Audio Brick; its S3 display header (DC on GPIO38) is a candidate on a
   board with no display, not verified. The pin is a board key, held to the pin rules (reserved,
   strapping, doubled) like every other. With `none` the marker is off and nothing is armed.
5. **The bring-up is a bench run** (`tools/embedded5-bringup-run.sh`, topic `embedded5-bringup`):
   it records `status` and `resources` before, while playing and after, starts a server with a
   quiet tone on the bench machine, and runs `CHORUS_EMBEDDED5_CYCLES` (10) rounds of server
   kill-and-restart with outages from 1 s to twice `reconnect_max_backoff_ms`. A round recovers
   when the endpoint plays new chunks within the longest backoff plus 10 s, a bound chosen for this
   run (the brief asks that it survive, not how fast). PASS needs amp=ok, playing, and every round
   recovered; heap, stack and FIFO are records that grade nothing. Pulling the Ethernet cable is
   the packet's manual step, not the script's.

## Consequences

- The sync against a Linux client stays `tools/endpoint-rig-run.sh`'s (the rig), and the marker's
  cross-check against a Linux client needs a marker on the Linux endpoint, which the Linux tier
  (goal 10) adds; until then the cross-check is between two ESP32-S3 endpoints.
- A stack figure near zero on the bench is a finding for the ADR that chose that stack (ADR 0058's
  4 KiB writer, ADR 0060's 16 KiB console), not a failed run.
