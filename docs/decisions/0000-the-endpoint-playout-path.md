# 0000: the endpoint plays through a jitter buffer whose device delay is the frames the I2S DMA consumed, counted and stamped in the interrupt, and corrects by inserting and dropping frames

- Status: accepted (goal 8, 2026-09-30)
- Decided by: the goal (brief section 12 item 2; audit A-9, A-12, A-18; goal 6's playout follow-ups)
- Implemented in: `firmware/src/playout.c`, `firmware/include/chorus/playout.h`,
  `firmware/src/sync_conf.c`, `firmware/main/esp_playout.c`, `firmware/main/esp_hal.c`
  (`hal_on_sent`, `chorus_esp_hal_attach_playout`), `firmware/main/app_main.c`,
  `firmware/src/session.c`, `firmware/tests/test_playout.c`

## Context

Until this goal the endpoint counted every audio chunk as received and dropped it (audit A-9): no
jitter buffer, no I2S writer, and a servo that never ran. The Linux client already closes the loop
(`crates/client-linux/src/sync.rs`): its error is

```
error_ns = (next_write_ts_ns + playout_latency_ns) - (client_now_ns + device_delay_ns + offset_ns)
```

with `device_delay_ns` from ALSA's `snd_pcm_delay`, and it corrects by changing what it writes
(frames inserted or dropped, a muted step for a hard resync), never when. The ESP32-S3 has no ALSA,
so the endpoint has to produce the device delay itself.

## What was read

- The ESP-IDF v6.1 I2S guide in the pinned tree (`docs/en/api-reference/peripherals/i2s.rst`,
  Apache-2.0, read 2026-09-30): the event callback "is an interrupt callback, so do not add complex
  logic, run floating operation, or call non-reentrant functions in the callback"; the 24-bit buffer
  on the ESP32-S3 is packed 3-byte samples; "IRAM Safe" and `CONFIG_I2S_ISR_IRAM_SAFE`.
- `components/esp_driver_i2s/include/driver/i2s_common.h` and `i2s_types.h` in the pinned tree
  (Apache-2.0, read 2026-09-30): `on_sent` ("Callback of data sent event ... The event data includes
  DMA buffer address and size that just finished sending data"); "DMA event callbacks can only be
  registered or deregistered before the channel is enabled"; `i2s_channel_preload_data` ("Only
  allowed to be called when the channel state is READY"; "when the `bytes_loaded` is smaller than
  the `size`, it means the DMA buffers are full"); `i2s_channel_tune_rate` and
  `i2s_tuning_config_t` ("Only allowed to be called when the channel state is READY, (i.e., channel
  has been initialized, but not started)").
- The esp_timer guide in the pinned tree (`docs/en/api-reference/system/esp_timer.rst`, "Obtaining
  Current Time", read 2026-09-30): `esp_timer_get_time` is usable "in tasks as well as in ISR
  routines"; `components/esp_timer/include/esp_timer.h`.
- `components/soc/esp32s3/include/soc/clk_tree_defs.h` in the pinned tree (Apache-2.0, read
  2026-09-30): `SYSTIMER_CLK_SRC_DEFAULT = SOC_MOD_CLK_XTAL`, `I2S_CLK_SRC_DEFAULT =
  SOC_MOD_CLK_PLL_F160M`, the 40 MHz XTAL as the external root clock.
- `components/esp_system/Kconfig` (`ESP_MAIN_TASK_STACK_SIZE`, default 3584; "If app_main() returns
  then this task is deleted and its stack memory is freed"), pinned tree, read 2026-09-30.
- PSA Certified Crypto API 1.2, "Overlap between parameters", section 5.4.4,
  https://arm-software.github.io/psa-api/crypto/1.2/overview/conventions.html, read 2026-09-30:
  "Output buffers can overlap with input buffers. In this event, the implementation must return the
  same result as if the buffers did not overlap."
- chorus's own `crates/client-linux/src/sync.rs`, `buffer.rs`, `run.rs` and `crates/sync/src/servo.rs`.
- No GPL source was opened.

## Decision

1. **The device delay is measured at the DMA.** The I2S TX `on_sent` interrupt calls
   `chorus_playout_on_dma_sent` with the size of the buffer that just finished; the hook adds its
   frames to a consumed count and stamps them there, in the interrupt, with the monotonic clock it
   is handed (`chorus_monotonic_now_ns`, i.e. `esp_timer_get_time`). The writer counts every frame
   it hands `i2s_channel_write`. The device delay is written minus consumed, less what of the buffer
   in flight has played since the stamp (bounded by one DMA buffer). The hook publishes through a
   sequence counter with 32-bit fields (a 32-bit store is one instruction on the S3; a 64-bit one is
   not) and takes no lock and does no float, as the guide requires.
2. **Written and consumed count the same frames from the start.** The path is attached before the
   amplifier's bring-up first starts the clock: the stopped channel's DMA ring is preloaded with
   silence that is counted as written. A buffer the DMA sends that the writer never handed over (the
   driver's auto-cleared zeros when the writer is late) is detected in the interrupt against the
   written count, counted as starvation, and treated as written silence, so the timeline moves on
   from it and the servo sees the step.
3. **The error and the servo are the Linux client's.** Same formula, same sign, the same
   `chorus_servo_update` (the C mirror of `crates/sync`), its threshold, clamp, interval, latency,
   mute and staleness from `config/sync.conf`. One addition: frames the corrector still owes are
   added to the error, because a step spread over several DMA buffers would otherwise be stepped
   again at the next tick (the Linux client applies a step inside one write, so it never owes one).
4. **Correction is insert and drop, not MCLK tuning.** ESP-IDF v6.1 offers `i2s_channel_tune_rate`,
   but only while the channel is READY, not started: tuning a running stream would mean stopping
   the clock, which is an audible gap and, for the amplifier, a clock loss. Insert and drop keeps one
   corrector shared in design with the Linux client, and the host test grades it. MCLK tuning
   (and the S3's TX FIFO sync counter) stay options for a later goal with a bench to compare them.
5. **The jitter buffer holds frames already in the I2S layout,** in internal RAM from the heap
   (`heap_caps_malloc(MALLOC_CAP_INTERNAL)`), 200 ms (`CHORUS_PLAYOUT_BUFFER_MS`, also the `buffer`
   the capabilities advertise, ASSUMED): 9,600 frames of two packed 24-bit slots, 57,600 bytes. Until
   the loop's first tick places the stream on the timeline, the writer holds the buffer and writes
   silence, and a full buffer gives up its oldest chunks.
6. **The second 64 KiB receive buffer is gone.** Records are opened in place over their own
   ciphertext (the PSA API permits the overlap; `firmware/tests/test_noise.c` checks it), which
   frees the static `plain[65535]` the jitter buffer needs.
7. **config/sync.conf is embedded and parsed on the endpoint** (A-12) with the same reader; the
   host session binary reads the same file, and `test_playout` holds every value to the Linux
   client's compiled constants. The two 64 KiB `chorus_conf_t` copies that the configuration parse
   put on a task's stack (a copy and a local in `endpoint_config.c`) are now borrowed from the heap
   for the parse and freed.
8. **Tasks and stacks.** The session runs in its own 32 KiB task (the decoders run inline in it:
   libopus is built with `VAR_ARRAYS`, and decoding the RFC 8251 CELT stereo vector took 22,056
   bytes of stack on the host, measured by painting a 512 KiB pthread stack and running the codec
   fixtures one by one: FLAC 10,048, Opus SILK mono 9,752, hybrid stereo 15,272; x86-64 at -O2, a
   host figure, not the Xtensa's); the writer runs at `configMAX_PRIORITIES - 2` with 4 KiB;
   `app_main` gets 8 KiB and returns. Every stack size is ASSUMED until the bench reads the
   high-water marks.
9. **`CONFIG_I2S_ISR_IRAM_SAFE` stays off** (the default): nothing writes flash during playback in
   this goal, so the interrupt and the clock read need not be in IRAM. OTA (goal 14) writes flash
   while playing and revisits it.

## Not chosen

- Stamping in the writer task after `i2s_channel_write` returns: that is when the driver accepted
  bytes, the signal the Linux module calls "the wrong signal", and task latency lands in it.
- A float or lock in the interrupt.
- A second buffer in external PSRAM: `firmware/endpoint-units.conf` rule 3 keeps every endpoint
  buffer internal.

## Consequences

- `firmware/tests/test_playout.c` (in `make firmware-check`, target `playout`) drives the path with
  a fake DMA on a fake monotonic clock and PCM that carries a frame counter, so it grades the true
  error at the pins as well as the servo's own: at +100, -100 and 0 ppm of DAC skew the correction
  settles within 5 ppm of the skew and both errors stay under 100 us over the last 20 s, with one
  hard resync (the acquisition); a 5 ms offset step, a 400 ms gap and a writer that lets the DMA
  run dry each recover. **This is a model, not timing evidence** (BRIEF section 3.1 rule 3); the
  amplifier's and I2S FIFO's own latency after the DMA is taken as zero (ASSUMED) until a bench
  measures it.
- Telemetry: `frames_played` counts only audio frames in DMA buffers the interrupt reported
  consumed; the published line adds `played=`, `underrun_frames=`, `late_chunks=`,
  `correction_ppm=` and `sync_error_ns=` (`not-applicable` without a playout path), and the wire
  telemetry's `sync_error_ns` carries the loop's last error instead of "unknown".
- A coded stream cannot yet be driven end to end through `session.c` from a real server: the server
  sends PCM only. The session hands decoded FLAC and Opus to the same `chorus_playout_offer` as PCM.
- The bench packet's EMBEDDED-5 session (goal 9) reads the free internal heap the binding logs at
  boot, the tasks' stack high-water marks, and the path's telemetry.
