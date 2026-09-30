# The bench packet: SOUND-2, RIG-3 and SYNC-4 on real hardware

What the owner buys, how it is wired, the exact commands each bench session runs, and the result
each one should print. Written in chorus goal 7 (2026-09-30) against proposal P4's recommended
tier, which Checkpoint K deferred: the buy list is **awaiting the owner's choice** (Needs item
"The bench buy list, awaiting your choice"), and nothing is ordered by the program (K4, K38).
Every session here is the owner's (brief §0.6); each has its own Needs item in the `## chorus`
section of NSchatz/shopkit `NEEDS-NOAH.md`. How a bench script turns a run into a report and a
pull request is `docs/bench.md`.

Nothing in this file is a measurement. Every expected result below is a criterion or an
`ASSUMED` estimate, and only a report under `docs/measurements/` with `Source: hardware` is timing
evidence (BRIEF.md section 3.1 rule 3).

## 1. The buy list (P4 Option B, recommended tier, awaiting choice)

Prices, sellers, ship-from and URLs are P4's, read 2026-09-30
(`docs/proposals/P4-bench-purchase.md`, Options A and B, with its "Price basis" column); confirm
every "snippet" and `ASSUMED` line in a browser before buying.

| # | Part | Sessions that need it | Qty | Line |
|---|---|---|---|---|
| A1 | Raspberry Pi 5 2GB (PiShop.us, 1 per order) | S0-S4 (endpoints A and B) | 2 | $130.00 |
| A2 | Raspberry Pi DAC+ (PCM5122, RCA line out) | S1-S4 | 2 | $59.90 |
| A3 | Raspberry Pi 27 W USB-C PSU | S0-S4 | 2 | $25.90 |
| A4 | Raspberry Pi SD card 32 GB | S0 | 2 | $39.90 |
| A5 | Behringer UMC202HD (2 in, 96 kHz capture used) | S1-S4 | 1 | $86.90 |
| A6 | SparkFun USB logic analyzer 24 MHz (sigrok fx2lafw) | not used by S1-S4 (see section 6) | 1 | $26.95 |
| A7 | Jumper wires | not used by S1-S4 | 2 | $3.90 |
| A8 | 2 x RCA to 1/4" TS cable, 1 x RCA Y splitter | S1-S4 | 1 set | $30.00 |
| B1-B10 | Esparagus Audio Brick x2 (pre-order, est. 2027-01-25), Waveshare ESP32-S3-ETH x2, PCM5102 DACs, PoE+ injector and splitter, 24 V supply, passive speaker pair, dummy loads, Cat6 | S5 and the EMBEDDED-5 and WIFI-7 packets of goals 8 and 9 | | $387.86 |
| | **Recommended total** | | | **$791.31** |

The Minimal tier (A1-A8, $403.45) is enough for sessions S0-S4. B-lines serve the embedded
sessions, which goals 8 and 9 turn into their own packets.

Also needed, not bought: a pair of headphones for listening through the UMC202HD's front
headphone output (`ASSUMED` the owner has one; any wired pair with a 1/4" plug or adapter), a
laptop or desktop with SSH to reach the Pis, and two free house-LAN ports (or one switch).

## 2. Wiring

The bench machine is **endpoint A**: a Pi 5 with a DAC+, which also runs `chorus-server` and has
the UMC202HD plugged into its USB. **Endpoint B** is the second Pi 5 with a DAC+, reached over SSH
from A (`CHORUS_SECOND_ENDPOINT=user@endpoint-b`, the form every rig script reads). Both Pis are
on the wired house LAN.

```
 endpoint A (Pi 5 + DAC+) --RCA L--> RCA-to-TS --> UMC202HD input 1 (combo jack, 1/4")
      |  USB                                          UMC202HD input 2 <-- RCA-to-TS <--RCA L-- endpoint B (Pi 5 + DAC+)
      +---- UMC202HD (USB audio, capture device)
      |
   Ethernet ---- house LAN ---- Ethernet
                                                     headphones -> UMC202HD front phones out (listening only)
```

- Only the left RCA channel of each DAC+ goes into the interface: input 1 is endpoint A, input 2
  is endpoint B. The rig reads the interface's left capture channel as A and right as B.
- Line level into the combo jacks: the DAC+ outputs 0-2 V RMS (Raspberry Pi DAC+ product brief,
  https://datasheets.raspberrypi.com/audio/dac-plus-hat-product-brief.pdf, search summary read
  2026-09-30), about +8 dBu at full scale; the UMC202HD's line inputs take up to +20 dBu (retailer
  specification summaries, https://www.bhphotovideo.com/c/product/1113600-REG/behringer_umc202hd_audiophile_2x2_24_bit_192_khz.html
  and https://higherhz.org/reviews/equipment/behringer-u-phoria-umc202hd/, search summaries read
  2026-09-30; confirm on the unit's own manual). Set both GAIN knobs fully down, both
  INST switches off, 48 V phantom **off**. The chirp is held at 0.25 of full scale
  (`config/measure.conf` `chirp_amplitude_ceiling`); raise interface gain only if a capture
  refuses for level, never the ceiling.
- **Self-calibration variant (session S2, step 1):** the Y splitter takes endpoint A's left RCA
  into both RCA-to-TS cables, so both inputs see the same signal; endpoint B is unplugged from the
  interface.
- Listening: the UMC202HD's direct-monitor knob fully to INPUT, headphones in the front jack.

## 3. The sessions

Every session starts on endpoint A in the chorus checkout of session S0, on a commit that is on
`origin/main` (`git pull --ff-only`), with a clean tree. Every run that produces evidence ends by
writing its report and opening a pull request from A (`docs/bench.md`); a later chorus goal
validates and merges it (K45). Device names below are examples: take the real ones from
`aplay -l` and `arecord -l` (step S0.6) and use `hw:<card>,0` forms.

### S0. The bench machine (one-time setup)

Needs: A1, A3, A4 (both Pis), the house LAN, a GitHub login on the owner's account.

1. Flash Raspberry Pi OS Lite (64-bit) onto both SD cards with Raspberry Pi Imager, enabling SSH
   and a user (names are the owner's; none is written here). Boot both Pis wired.
2. On endpoint A: `sudo apt update && sudo apt install -y git gh build-essential pkg-config libasound2-dev curl`.
3. On endpoint A: `curl https://mise.run | sh`, then `echo 'eval "$(~/.local/bin/mise activate bash)"' >> ~/.bashrc` and open a new shell.
4. On endpoint A: `gh auth login` (GitHub.com, HTTPS, log in with a browser), then
   `gh repo clone NSchatz/chorus && cd chorus && mise install && mise use rust@1.98.1` (the repo's
   `mise.toml` pins the tools; `rust-toolchain.toml` pins Rust 1.98.1).
5. On endpoint A: `cargo build --release --workspace`; then copy the client to B:
   `ssh-copy-id user@endpoint-b`, then `ssh user@endpoint-b mkdir -p .local/bin` and
   `scp target/release/chorus-client user@endpoint-b:.local/bin/` (B needs `chorus-client` on its
   `PATH`, built from the same commit, and passwordless SSH from A, per `docs/bench.md`).
6. Enable the DAC on both Pis: add `dtoverlay=hifiberry-dacplus-std` to `/boot/firmware/config.txt`
   (the overlay the Raspberry Pi audio documentation names for the DAC+,
   https://www.raspberrypi.com/documentation/accessories/audio.html, search summary read
   2026-09-30; confirm there), reboot, and run `aplay -l` on both and `arecord -l` on A.

Expected: `gh auth status` says logged in; `cargo build` ends `Finished`; `aplay -l` lists the DAC+
card on both Pis; `arecord -l` on A lists the UMC202HD ("U-PHORIA UMC202HD" or similar).
What changes: nothing in the repo; every later session can run. Record the card names in the
Needs item (no hostnames, no addresses).

### S1. SOUND-2: first sound and the ten-minute run

Needs: S0, endpoint A with A2, A5, headphones. BRIEF.md section 8 item 2: "clean audio, plausible
reported DAC delay, stable buffer".

1. The software check first (nothing is recorded): `make verify-null-device` (it runs on the
   ALSA `null` device whatever `CHORUS_CLIENT_DEVICE` says). Then the device-class checks of
   `docs/sound-2.md` on the DAC+, which open one pull request per script (four:
   `sound2-stream-end-and-loss`, `sound2-start-fill`, `sound2-delay-log-shape`,
   `sound2-overflow`; a failing script still opens its PR, with `Result: FAIL`):
   ```
   CHORUS_BENCH_PR=1 CHORUS_BENCH_DEVICE_NOTE='Raspberry Pi 5 2GB, Raspberry Pi DAC+' \
CHORUS_CLIENT_DEVICE=hw:<dac card>,0 make verify-device
   ```
   Listen on the headphones while it plays.
2. The evidence run, ten minutes, report and pull request:
   ```
   CHORUS_BENCH_PR=1 CHORUS_BENCH_DEVICE_NOTE='Raspberry Pi 5 2GB, Raspberry Pi DAC+' \
CHORUS_CLIENT_DEVICE=hw:<dac card>,0 ./tools/ten-minute-run.sh
   ```

Expected: step 1 exits 0, the audio is clean (no clicks, dropouts or pitch wobble heard) and
four pull requests `bench/<date>-sound2-*` open, each `Result: PASS`;
step 2's log grades with at least 600 graded seconds, zero underruns and no rate change
(`chorus-delaylog-check --min-graded-seconds 600 --require-zero-underruns --require-no-rate-change`),
the reported delay inside the configured buffer bounds (`config/transport.conf`), and a pull
request `bench/<date>-sound2-ten-minute` opens. "Plausible DAC delay": the logged delay is the
buffer the client configured plus a device delay of a few milliseconds (`ASSUMED`; the report
records what the DAC+ reports). What changes: `docs/sound-2.md`'s three NOT PASSED rows.

### S2. RIG-3: the rig proves itself, then a free-run baseline

Needs: S0, both endpoints wired per section 2. BRIEF.md section 8 item 3: the rig, "a free-run
drift baseline between two clients".

1. Self-calibration (Y splitter, section 2): the capture run with endpoint A in both inputs:
   ```
   CHORUS_BENCH_PR=1 CHORUS_BENCH_DEVICE_NOTE='self-calibration: endpoint A into both inputs (Y splitter), UMC202HD' \
   CHORUS_SECOND_ENDPOINT=user@endpoint-b CHORUS_CAPTURE_DEVICE=hw:<umc card>,0 \
   CHORUS_CLIENT_DEVICE=hw:<dac card>,0 ./tools/measure/capture-run.sh
   ```
   Expected: the lag between the
   inputs resolves at 0 us within the rig's resolution (about 10 us at 96 kHz, BRIEF.md section
   10); anything else is the interface's own inter-channel skew, recorded in the report.
2. Rewire to A on input 1 and B on input 2. The capture run, both endpoints synced:
   ```
   CHORUS_BENCH_PR=1 CHORUS_BENCH_DEVICE_NOTE='2 x Raspberry Pi 5 2GB with Raspberry Pi DAC+, UMC202HD' \
   CHORUS_SECOND_ENDPOINT=user@endpoint-b CHORUS_CAPTURE_DEVICE=hw:<umc card>,0 \
   CHORUS_CLIENT_DEVICE=hw:<dac card>,0 ./tools/measure/capture-run.sh
   ```
3. The free-run baseline, correction disabled on both clients (audit A-4):
   ```
   CHORUS_BENCH_PR=1 CHORUS_BENCH_DEVICE_NOTE='2 x Raspberry Pi 5 2GB with Raspberry Pi DAC+' \
   CHORUS_SECOND_ENDPOINT=user@endpoint-b CHORUS_CLIENT_DEVICE=hw:<dac card>,0 \
       ./tools/measure/free-run-run.sh
   ```

Expected: step 2 resolves at least `min_resolved_windows` (4) windows above the confidence floor
(0.6) and writes a lag distribution; step 3 writes the free-run drift between the two Pi crystals
with at least 30 points over at least 60 s and a confidence half-width at most 1.0 ppm
(`config/measure.conf`); three pull requests open (`bench/<date>-rig3-capture`, its second run suffixed `-2`, and
`bench/<date>-rig3-free-run`). What changes: the free-run baseline
(`docs/measurements/free-run-baseline.conf`) becomes `source = hardware`.

### S3. SYNC-4: the hour

Needs: S2 passed (the rig is trusted). BRIEF.md section 8 item 4; SYNC-4 AC-1 in
`docs/verification-record.md`: "median inter-device error below 0.5 ms as measured by the RIG-3
harness and ... no hard resync after the first minute".

```
CHORUS_BENCH_PR=1 CHORUS_BENCH_DEVICE_NOTE='2 x Raspberry Pi 5 2GB with Raspberry Pi DAC+, UMC202HD' \
CHORUS_SECOND_ENDPOINT=user@endpoint-b CHORUS_CAPTURE_DEVICE=hw:<umc card>,0 \
CHORUS_CLIENT_DEVICE=hw:<dac card>,0 ./tools/sync-hour-run.sh
```

It runs 3900 s (65 minutes: a 60 s settle, then 6 captures of 30 s spread over the hour,
`config/sync.conf`). Leave the bench alone while it runs.

Expected: median inter-device error below 0.5 ms (the BRIEF.md section 2.2 same-room bound) and
zero hard resyncs after the first minute; a pull request `bench/<date>-sync4-hour` opens. If the
median is above 0.5 ms, the report still opens; the next goal reads it (BRIEF.md section 9: never
chase a bad number with servo aggression).

### S4. PRODUCT-6: the three-day soak

Needs: S3 passed; the bench undisturbed for three days.

```
CHORUS_BENCH_PR=1 CHORUS_SOAK_SECONDS=259200 CHORUS_SECOND_ENDPOINT=user@endpoint-b \
CHORUS_CAPTURE_DEVICE=hw:<umc card>,0 CHORUS_CLIENT_DEVICE=hw:<dac card>,0 \
    ./tools/soak-run.sh
```

Expected: the sync bound of S3 still held at the end and no unexplained resync (PRODUCT-6 AC-4);
a pull request `bench/<date>-product6-soak` opens.

### S5. The ESP32-S3 on ESP-IDF v6.1: boot check (the owner's own board)

Needs: one of the owner's ESP32-S3 boards with at least 2 MB of flash (the image's application
partition is 1.5 MB, ADR 0044), a USB cable, ESP-IDF v6.1 on endpoint A or a laptop (the pin in
`firmware/config/endpoint.conf`). No bought part.

This is the goal-6 follow-up: the first run of the v6.1 image on silicon. There is no chorus
flashing tool yet (goal 9 adds it behind `CHORUS_OWNER_AT_BENCH`), so this uses ESP-IDF's own
tool, run by the owner only:

```
. <esp-idf v6.1>/export.sh
make firmware-image                     # builds firmware/build/image
cd firmware && CHORUS_OWNER_AT_BENCH=1 idf.py -B build/image -D SDKCONFIG=build/image/sdkconfig -p /dev/ttyACM0 flash monitor
```

Never run `espefuse` or any eFuse command (BRIEF.md section 3.1 rule 2); the image guard in
`make gate` already proves the image burns none.

Expected: the boot log names ESP-IDF v6.1, the chorus app starts, and it stops at
`the committed configuration is refused; no clock is started` (audit A-10: the committed endpoint
configuration has no link or amplifier map until goals 8 and 9); no pin is driven. Paste the log
lines from the ESP-IDF version line to that refusal into the Needs item, with any `MAC:` line
deleted (K27). What changes: goal 8 knows the v6.1 image boots on this board. From goal 8 the
image also starts its serial console before that refusal (ADR 0059), so S6 below can run on the
same board right after.

### S6. The ESP32-S3's decode cost, on the chip (the owner's own board, after S5)

Needs: the board from S5, flashed with an image from the same checkout, on USB; S0's checkout on
the machine it is plugged into. No bought part. Goal 8 adds this (ADR 0059): the image carries one
FLAC and one Opus stream from `fixtures/codec` and times their decode on its monotonic clock.

```
CHORUS_ESP32S3_PORT=/dev/ttyACM0 CHORUS_BENCH_PR=1 tools/decode-cost-run.sh
```

Expected: the console answers `help`; the report `embedded5-decode-cost-<date>` is MEASURED with
`flac_cpu_fraction` and `opus_cpu_fraction` (the fraction of one core real-time decoding takes on
the S3), both decodes matching their references, and `console_stack_free_bytes` above zero; a pull
request `bench/<date>-embedded5-decode-cost` opens. Nothing is graded against a bound: the figures
feed the playout and DSP budgets. What changes: goal 6's host-only decode cost gets its S3
counterpart; if the stack figure is small, the console's 16 KB stack (ASSUMED) grows.

## 4. Order and what each session unblocks

S0 first; S1 needs one Pi; S2 needs both and the interface; S3 needs S2's rig trusted; S4 after
S3. S5 is independent (the owner's own board) and can run any time; S6 follows S5 on the same
board. None of these blocks a chorus
goal (K7): each goal that can use a result re-checks for its pull request.

## 5. How results come back

Each evidence run (S1-S4) commits its report on `bench/<date>-<topic>` and opens a pull request
from endpoint A (`docs/bench.md`); the owner does nothing else. If the report or pull-request half
fails after a measurement, nothing is lost: fix the cause and run the same script with
`--report-from <run directory>` (printed at the start) from the same commit. S0 and S5 are answered in their
Needs items.

## 6. Not in this packet

- The GPIO marker cross-check with the logic analyzer (BRIEF.md section 10's digital
  cross-check): no endpoint drives a marker pin yet and no chorus tool reads a sigrok capture;
  goal 8 (endpoint playout) is the first goal that can add one.
- EMBEDDED-5 (the S3 + W5500 line-level sync against a Linux client, `tools/endpoint-rig-run.sh`)
  and WIFI-7 (`tools/wireless-characterization-run.sh`): they need goal 8's link and playout and
  goal 9's amplifier map; those goals write their packets. Both scripts now drive the endpoint
  console (goal 8, ADR 0059), and the rig grades AC-3's produced rate with `chorus-measure rate`.
- The production host's SCHED_FIFO wakeup-jitter run: a homelab-side Needs item, not a bench
  session (`docs/measurements/host-wakeup-jitter.md`). Its output files come back the bench way:
  committed under `docs/measurements/raw/production-wakeup-<date>/` on a
  `bench/<date>-production-wakeup` branch with a pull request, from which a chorus goal writes the
  report (there is no bench script for it yet).
- `make verify-host` (the host contract and the spin test): they need a real-time priority grant,
  a host setting rather than bench hardware; chorus goal 10 (the Linux endpoint tier) runs them on
  the Linux endpoint it packages.
