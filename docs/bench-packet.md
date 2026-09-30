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

Expected: the boot log names ESP-IDF v6.1, the chorus app starts its serial console (ADR 0060),
and it stops before any link or session because the amplifier map is still `unknown` (goal 9
fills it): either the amplifier bring-up's refusal naming the first `unknown` key, or
`the hardware could not be brought up; the output stage stays dead` if the I2C driver rejects the
unset address. From goal 8 the default image is the `brick-s3-wired` board profile (ADR 0057, its
board model ASSUMED until the boards Needs item is answered), so it configures that board's pins
as outputs (the amplifier power-down line driven low, the I2S pins); on a bare development board
wire nothing to them for this session. Paste the log lines from the ESP-IDF version line to the
stop into the Needs item, with any `MAC:` line deleted (K27). What changes: goal 9 knows the v6.1
image boots on this board, and S6 below can run on the same board right after, through the
console.

### S6. The ESP32-S3's decode cost, on the chip (the owner's own board, after S5)

Needs: the board from S5, flashed with an image from the same checkout, on USB; S0's checkout on
the machine it is plugged into. No bought part. Goal 8 adds this (ADR 0060): the image carries one
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

### S7. EMBEDDED-5: the ESP32-S3 endpoint plays, syncs and survives abuse (after S0; the Audio Brick)

The EMBEDDED-5 packet (chorus goal 9; BRIEF.md section 8 item 5: "ESP32-S3 firmware: I2S out,
TAS5825M alive, network playback, sync core ported and measured against a Linux client. Success:
the embedded endpoint syncs comparably to Linux and survives disconnect/reconnect abuse
unattended"). Four steps: flash, watch it boot, the bring-up and abuse run, then the sync on the
rig. A fifth, the GPIO marker cross-check, needs a logic analyzer and a free pin.

**Needs** (from section 1; nothing is ordered by the program):

| Step | What | Lines |
|---|---|---|
| S7.1-S7.3 | one Esparagus Audio Brick (ESP32-S3, TAS5825M, W5500): the `brick-s3-wired` board profile | B1 (+ B2 shipping) |
| S7.1-S7.3 | its power: a 24 V DC supply, or PoE+ through the injector and splitter (the compact speaker's path, K90) | B7, or B5 + B6 |
| S7.1-S7.3 | a passive speaker pair on its terminals, and a USB-C data cable to endpoint A | B8 |
| S7.1-S7.3 | Ethernet to the house LAN (or to B5's data-in port) | B10 |
| S7.4 | S2's trusted rig (endpoint A, endpoint B, the UMC202HD) and a load and divider for the Brick's speaker-level output | A1-A5, A8, B9 |
| S7.5 | the logic analyzer and jumpers, and a second ESP32-S3 endpoint (a second B1, or B3) | A6, A7 |

The board model is **ASSUMED**: `firmware/boards/brick-s3-wired.conf` names P1's reference board
until the Needs item "Your ESP32-S3 boards: module markings and a read-only chip report" is
answered. If your board is not an Audio Brick, stop and answer that item first: a different board
needs its own profile (pins, amplifier) before it is flashed.

**Wiring** (`brick-s3-wired`; pins from the maker's Apache-2.0 configuration, cited in
`firmware/config/endpoint.conf`; everything below is on the board, nothing is jumpered):

```
 24 V supply (B7) --or-- PoE+ injector (B5) -> Cat6 -> splitter (B6) -> 24 V out --+
                                                        splitter data out --Cat6--+--> Brick RJ45 (W5500)
                                                                                   +--> Brick DC in (5-26 V)
 Brick speaker terminals L+/L-, R+/R- ---- speaker pair (B8), 4-8 ohm
 Brick USB-C ---- endpoint A (flashing and the serial console, /dev/ttyACM0)
```

- Speakers or loads only on the speaker terminals: the TAS5825M's outputs are bridge-tied (both
  terminals of a channel switch; neither is ground). Never join a speaker terminal to endpoint A,
  the interface or any ground.
- The first power-up is with the speakers connected and the volume at the image's floor: the
  analog gain is the part's lowest setting (`amp_analog_gain_db = 0.0` above the lowest,
  `firmware/config/endpoint.conf`), and the endpoint refuses anything above that ceiling.

#### S7.1 Build and flash (the owner's act)

On endpoint A, in S0's checkout, with ESP-IDF v6.1 exported (the pin in
`firmware/config/endpoint.conf`):

```
. <esp-idf v6.1>/export.sh
git pull --ff-only
make firmware-image                                   # builds firmware/build/image (brick-s3-wired)
tools/firmware-flash.sh --print --port /dev/ttyACM0   # shows the esptool command, runs nothing
CHORUS_OWNER_AT_BENCH=1 tools/firmware-flash.sh --port /dev/ttyACM0
```

`tools/firmware-flash.sh` refuses unless you set `CHORUS_OWNER_AT_BENCH=1` on its own command line
(nothing in the repository sets it), and it never runs an eFuse, Secure Boot, Flash Encryption or
anti-rollback command. Never run `espefuse` (BRIEF.md section 3.1 rule 2).

#### S7.2 Watch it boot, and keep the log

```
cd firmware && idf.py -B build/image -p /dev/ttyACM0 monitor | tee ../embedded5-boot.log
```

(`Ctrl+]` leaves the monitor; delete any `MAC:` line from the log, K27.) **Expected**, in this
order (tags `chorus`, `chorus-console`, `chorus-playout`, `chorus-marker`; timestamps differ):

1. ESP-IDF's own boot lines, including `ESP-IDF:          v6.1`.
2. `chorus: board profile=brick-s3-wired model="Sonocotta Esparagus Audio Brick (ESP32-S3), TAS5825M, W5500" status=ASSUMED needs_item="Your ESP32-S3 boards: module markings and a read-only chip report" link=wired`
3. `chorus-console: console up: power-save, server, status, decode-cost, resources (values are runtime only)`
4. `chorus-marker: marker off (pin_marker = none)`
5. `chorus-playout: jitter buffer 57600 bytes (9600 frames), internal RAM free after it: <n> bytes`
6. The amplifier's bring-up (the TAS5825M's datasheet sequence, ADR 0064):
   `chorus: the amplifier at I2C address 0x4c answered as device 0x95, reported no fault, took 0.000 dB of analog gain against a ceiling of 0.000 dB, and reached Play after its clock was applied in Deep Sleep`.
   The address 0x4C assumes the ADR pin strapped to ground (`ASSUMED`,
   `firmware/config/endpoint.conf`); a part that does not answer there is refused with the output
   stage dead, naming the address.
7. `chorus: chorus-endpoint: link=... transport=wired ... amp=ok amp_fault_bits=0x00 ...` (the
   telemetry line), then
   `chorus: link=wired phy=w5500 status=up spi=spi2 clock_mhz=20 sclk=12 mosi=11 miso=13 cs=10 int=6 address_timeout_ms=30000`.
8. Silence from the speakers: the image dials its committed server address (`127.0.0.1:4010`,
   itself) until S7.3 points it at a real one.

What changes with the answer: nothing to edit if it matches. If the amplifier's bring-up line is a
refusal instead, it names the key and the datasheet page to re-check in
`firmware/config/endpoint.conf`; paste the lines into the Needs item and stop here.

#### S7.3 The bring-up and abuse run (unattended, about 10 minutes)

With the monitor closed (the script needs the serial port) and the Brick still on the LAN:

```
CHORUS_ESP32S3_PORT=/dev/ttyACM0 CHORUS_EMBEDDED5_BOOT_LOG=embedded5-boot.log \
    CHORUS_BENCH_PR=1 tools/embedded5-bringup-run.sh
```

It starts `chorus-server` on endpoint A with a quiet 440 Hz tone, points the endpoint at it over
the console, records `status` and `resources` (free heap internal and PSRAM, least-ever free, each
task's least free stack, the FIFO after the writer, the marker's counts) before, while playing and
after, then kills and restarts the server 10 times (`CHORUS_EMBEDDED5_CYCLES`), with outages from
1 s to 10 s (twice `reconnect_max_backoff_ms`), and requires the endpoint to be playing again each
time without anyone touching it. **Expected:** you hear the tone, with a gap at each outage; the
report `embedded5-bringup-<date>` is **PASS** with `amp_status = ok`, `cycles_recovered = 10 of
10`, and figures in `heap_internal_min_free`, `stack_min_free` and `fifo_us` (records, not graded);
a pull request `bench/<date>-embedded5-bringup` opens. Then, by hand: pull the Brick's Ethernet
cable for 30 s and plug it back; `status` on the console (`printf 'status\r\n' > /dev/ttyACM0`
while `idf.py monitor` shows the reply, or rerun the script) should show `audio=running` again
within 15 s. Note the result in the Needs item. What changes: a stack figure near zero grows that
task's stack (ADRs 0058 and 0060); a FAIL names the round and the status line it stopped on.

#### S7.4 The sync against a Linux client, on the rig (after S2)

The rig compares the Brick with endpoint B (a Linux client) on the UMC202HD, as SYNC-4 compares
two Pis. The Brick's output is speaker level and bridge-tied, and each terminal sits at a DC level
of about half the supply at idle (both legs of a bridge-tied class-D stage switch around the
middle of the supply; `ASSUMED` from the output-stage type, confirm with the multimeter below), so
it goes into the interface through a load, DC-blocking capacitors and a divider (B9), never
directly. One channel (L) is enough:

```
 Brick L+ --+-------------------- 8 ohm 50 W load --------------------+-- Brick L-
            |                                                        |
          10 uF 50 V (film or bipolar)                             10 uF 50 V
            |                                                        |
          9.1 k                                                    9.1 k
            |                                                        |
 TRS tip ---+--- 1 k --- TRS sleeve --- 1 k ---+------------------- TRS ring
                                               (the plug into UMC202HD input 1)
 endpoint B (Pi 5 + DAC+) RCA L --> RCA-to-TS --> UMC202HD input 2 (as in S2)
```

- Before the plug goes into the interface, with the Brick powered and playing nothing: measure DC
  between tip and sleeve and between ring and sleeve with the multimeter. Both must read below
  0.1 V; if not, a capacitor is missing or wrong and nothing is plugged in.
- `ASSUMED` values: about 10:1 on each leg into the interface's balanced line input, sized for the
  image's lowest analog gain and the chirp at 0.25 of full scale; the capacitors' 1.6 Hz corner
  is far below the chirp. That the UMC202HD's combo inputs take a balanced TRS line signal is
  `ASSUMED` from its retailer specifications (section 2): confirm in its manual. Start with the
  GAIN knob fully down.
- Speakers off the Brick for this step (the load replaces them).

```
CHORUS_ESP32S3_PORT=/dev/ttyACM0 CHORUS_SECOND_ENDPOINT=<user>@<endpoint-b> \
    CHORUS_CAPTURE_DEVICE=hw:<card>,0 CHORUS_BENCH_PR=1 tools/endpoint-rig-run.sh
```

**Expected:** the report `embedded5-endpoint-rig-<date>` is **PASS** with
`median_of_medians_us` below `bound_us` (500, `config/transport.conf` `wired_bound_us`: SYNC-4's
bound) and `produced_rate` inside the servo's authority (AC-3); a pull request
`bench/<date>-embedded5-endpoint-rig` opens. What changes: EMBEDDED-5's "syncs comparably to
Linux" is graded; a FAIL is the finding the sync engine's next goal starts from.

#### S7.5 The GPIO marker cross-check (optional: a logic analyzer and a free pin)

The firmware can drive a marker pin at every server-timeline second (ADR on the marker,
`firmware/main/esp_marker.c`), but `pin_marker` is `none` on the Brick: its maker documents no
free broken-out GPIO (the S3 display header's GPIO38 is a candidate on a board with no display,
unverified). If you find a free pin on both S3 endpoints: set `pin_marker = <gpio>` in each one's
board profile, rebuild and flash both (S7.1), wire each marker pin and a common ground to the
analyzer's D0 and D1, and capture a minute:

```
sigrok-cli -d fx2lafw --config samplerate=24m --channels D0,D1 --time 60s -O csv -o marker.csv
```

**Expected:** one edge per second on each channel, the same level on both at each second, the
edge delta between D0 and D1 at most tens of microseconds once both are synced; each endpoint's
`resources` shows `marker_edges` rising and `marker_missed` at 0. No chorus tool reads the
capture yet and a Linux client has no marker until the Linux tier (chorus goal 10) adds one: put
`marker.csv` in the Needs item, and a later goal turns it into a report.

## 4. Order and what each session unblocks

S0 first; S1 needs one Pi; S2 needs both and the interface; S3 needs S2's rig trusted; S4 after
S3. S5 is independent (the owner's own board) and can run any time; S6 follows S5 on the same
board. S7 needs an Audio Brick (B1): S7.1-S7.3 after S0 alone, S7.4 after S2's rig is trusted,
S7.5 whenever a logic analyzer and a free pin exist. None of these blocks a chorus
goal (K7): each goal that can use a result re-checks for its pull request.

## 5. How results come back

Each evidence run (S1-S4, S6, S7.3, S7.4) commits its report on `bench/<date>-<topic>` and opens a pull request
from endpoint A (`docs/bench.md`); the owner does nothing else. If the report or pull-request half
fails after a measurement, nothing is lost: fix the cause and run the same script with
`--report-from <run directory>` (printed at the start) from the same commit. S0 and S5 are answered in their
Needs items.

## 6. Not in this packet

- A reader for the GPIO marker's capture (S7.5 records it as a CSV only) and a marker on the
  Linux client: the Linux tier (chorus goal 10) adds the Linux marker, and the goal that has a
  capture to read writes its report.
- WIFI-7 (`tools/wireless-characterization-run.sh`, the compact speakers' Wi-Fi tier): it needs a
  wireless network the repository declares unknown and a provisioning path (chorus goal 14).
- The production host's SCHED_FIFO wakeup-jitter run: a homelab-side Needs item, not a bench
  session (`docs/measurements/host-wakeup-jitter.md`). Its output files come back the bench way:
  committed under `docs/measurements/raw/production-wakeup-<date>/` on a
  `bench/<date>-production-wakeup` branch with a pull request, from which a chorus goal writes the
  report (there is no bench script for it yet).
- `make verify-host` (the host contract and the spin test): they need a real-time priority grant,
  a host setting rather than bench hardware; chorus goal 10 (the Linux endpoint tier) runs them on
  the Linux endpoint it packages.
