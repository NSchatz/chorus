# The bench packet: SOUND-2, RIG-3 and SYNC-4 on real hardware

What the owner buys, how it is wired, the exact commands each bench session runs, and the result
each one should print. Written in chorus goal 7 (2026-09-30) against proposal P4's recommended
tier, which Checkpoint K deferred: the buy list is **awaiting the owner's choice** (Needs item
"The bench buy list, awaiting your choice"), and nothing is ordered by the program (K4, K38).
Every session here is the owner's (brief §0.6); each has its own item in the owner's queue (issues
in NSchatz/goals; `goals needs add`, `/goals:needs`). How a bench script turns a run into a report and a
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
| A6 | SparkFun USB logic analyzer 24 MHz (sigrok fx2lafw) | S7.5 only (the GPIO marker) | 1 | $26.95 |
| A7 | Jumper wires | S7.5 only | 2 | $3.90 |
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

Needs: one of the owner's ESP32-S3 boards with 8 MB of flash or more (since goal 14 the image's
layout is two 3 MiB update slots, `firmware/partitions.csv`, and the build declares 8 MB; the
board's flash size is ASSUMED until this session's read-only chip report says; a board with less
cannot hold this layout, and that is a finding to paste back, not something to work around), a USB cable, ESP-IDF v6.1 on endpoint A or a laptop (the pin in
`firmware/config/endpoint.conf`). No bought part.

This is the goal-6 follow-up: the first run of the v6.1 image on silicon. It flashes with chorus's
guarded tool (goal 9, `tools/firmware-flash.sh`), which refuses unless you set
`CHORUS_OWNER_AT_BENCH=1` on its command line, run by the owner only:

```
. <esp-idf v6.1>/export.sh
make firmware-image                     # builds firmware/build/image
tools/firmware-flash.sh --print --port /dev/ttyACM0     # shows the esptool command, runs nothing
CHORUS_OWNER_AT_BENCH=1 tools/firmware-flash.sh --port /dev/ttyACM0
cd firmware && idf.py -B build/image -p /dev/ttyACM0 monitor
```

Never run `espefuse` or any eFuse command (BRIEF.md section 3.1 rule 2); the image guard in
`make gate` already proves the image burns none.

Expected: the boot log names ESP-IDF v6.1, the chorus app starts its serial console (ADR 0060),
and it then brings the amplifier up with the register map goal 9 read from TI's datasheet
(ADR 0064). On a bare development board with no amplifier, or a board
whose ADR strap is not the ASSUMED 0 ohm one, the bring-up stops by name with
`amp=amplifier-did-not-answer` and the output stage dead (PDN low, no clock); on a TAS5825M board
it reaches Play, or stops naming the fault register and its bits, then stops at the link or the
session if no server is configured. From goal 8 the default image is the `brick-s3-wired` board
profile (ADR 0057, its board model ASSUMED until the boards Needs item is answered), so it
configures that board's pins as outputs (the amplifier power-down line driven low then high, the
I2S pins, the I2C pins); on a bare development board wire nothing to them for this session. Paste
the log lines from the ESP-IDF version line to the stop into the Needs item, with any `MAC:` line
deleted (K27). What changes: goal 9 knows the v6.1
image boots on this board, and S6 below can run on the same board right after, through the
console.

### S6. The ESP32-S3's decode cost and DSP chain cost, on the chip (the owner's own board, after S5)

Needs: the board from S5, flashed with an image from the same checkout, on USB; S0's checkout on
the machine it is plugged into. No bought part. Goal 8 adds this (ADR 0060): the image carries one
FLAC and one Opus stream from `fixtures/codec` and times their decode on its monotonic clock. Goal
12 adds the DSP chain to the same run (`dsp-cost`, ADR 0086): the image runs the endpoint's chain in
four configurations (flat; all-on: tone, loudness, speech, night and eight room-EQ filters; the LFE
member of a 2.1 set; the two-way split) over one second of a generated 48 kHz stereo signal, times
the chain calls on the same clock, and checks the output against the checksum the host computed.
Take it with no stream playing: the console shares the chip with the session.

```
CHORUS_ESP32S3_PORT=/dev/ttyACM0 CHORUS_BENCH_PR=1 tools/decode-cost-run.sh
```

Expected: the console answers `help`; the report `embedded5-decode-cost-<date>` is MEASURED with
`flac_cpu_fraction` and `opus_cpu_fraction` (the fraction of one core real-time decoding takes on
the S3), both decodes matching their references, `console_stack_free_bytes` above zero, and
`dsp_flat_cpu_fraction`, `dsp_all_on_cpu_fraction`, `dsp_sub_cpu_fraction` and
`dsp_two_way_cpu_fraction` (the fraction of one core the chain takes in each configuration), every
`dsp_*_output_matches` `yes` (`bit exact` may say `no`: the chip's libm may round a last place
differently, which the tolerance allows); a pull request `bench/<date>-embedded5-decode-cost`
opens. The run takes under a minute after the console answers. Nothing is graded against a bound:
the figures feed the playout and DSP budgets. If the reply is `error dsp-cost
reason=chain-failed detail="no memory for a 72680 byte chain"`, the board has not the internal RAM
for a second chain beside the playout path's; the report is FAIL with that line, and the decode
figures are still in it. What changes: goal 6's host-only decode cost gets its S3 counterpart and
the DSP chain gets its first figure anywhere on the chip, so a headroom claim for the chain can
cite a report; if the stack figure is small, the console's 16 KB stack (ASSUMED) grows.

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

The firmware can drive a marker pin at every server-timeline second (ADR 0065,
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

### S8. The TV capture session: a TV's sound into the hub, measured (goal 13; after S0 and S1)

The TV path (chorus goal 13, BRIEF.md section 5.7, P2 Option A: stereo LPCM over optical or an
ARC extractor, CEC through the kernel) is built and tested only against modelled TVs, a fake CEC
bus and simulated networks. Every TV-dependent value in it is `ASSUMED` until this session
answers it, and each step below names the value its answer replaces. Nothing here flashes,
burns or changes a TV setting beyond its own audio menu; run it at the owner's pace, one TV at a
time.

**First, the TVs (no hardware).** Answer the Needs item "The three TVs: model, eARC port, optical
out and audio menu" for each of the owner's three TVs before buying anything: the model (from
the label or Settings > System > About), which HDMI port says ARC or eARC, whether it has an
optical (TOSLINK) output, and the audio menu's choices for its digital output (on a Roku TV:
Settings > System > Audio > S/PDIF and ARC, P2 [S1]). Until it is answered every TV value in
the code and the docs is `ASSUMED` (ADRs 0087, 0088, 0090, 0091). What changes: which TV gets
optical and which needs the ARC extractor (B-lines below), and whether one TV is skipped (no
optical and no ARC).

**Needs** (prices, sellers and URLs are P2's, re-checked 2026-09-30,
`docs/proposals/P2-theater-scope.md` section "Re-check" and its source list; confirm each in a
browser before buying; nothing is ordered by the program, K4):

| # | Part | Steps | Qty | Line |
|---|---|---|---|---|
| A1, A3, A4 | Raspberry Pi 5 2GB, its PSU and SD card, as the hub (section 1, P4) | all | 1 | from section 1 |
| T1 | HiFiBerry Digi+ I/O (optical and coax in and out, Pi HAT), P2 [H7] https://www.hifiberry.com/shop/boards/hifiberry-digi-io/ (**NON-US EXCEPTION**, Swiss seller `ASSUMED`; PiShop.us US$44.75 not re-read) | S8.1-S8.8 | 1 | US$54.90 |
| T2 | or a DIR9001 receiver module (TI DIR9001, P2 [H5], [H6]) instead of T1 | S8.1-S8.8 | 1 | `ASSUMED` US$10-20 |
| T3 | OREI BK-931 HDMI ARC/eARC audio extractor, for a TV without optical, P2 [H4] https://www.orei.com/products/8k-hdmi-or-earc-audio-extractor-bk-931 | S8.8, S8.9 | 1 | US$109.99 |
| T4 | Pulse-Eight USB-CEC adapter, only if the hub's own HDMI port cannot reach the TV, P2 [H8] https://www.pulse-eight.com/p/104/usb-hdmi-cec-adapter (**NON-US EXCEPTION**, UK seller `ASSUMED`) | S8.2, S8.9 | 1 | US$48.08 |
| T5 | a TOSLINK cable and a micro-HDMI to HDMI cable (the Pi 5's port) | all | 1 each | `ASSUMED` US$10-15 |
| A2, A5, A8 | S1's DAC+, the UMC202HD and its cables, for the lip-sync step (S8.7) | S8.7 | | from section 1 |

Also needed, not bought: a phone that films at 240 frames a second (BRIEF.md section 10), a
second wired host on the house LAN for S8.6 (endpoint A of S0, or any Linux machine).

**Wiring:**

```
 TV optical out --TOSLINK--> Digi+ I/O optical in (on the hub, a Pi 5)
 TV HDMI (any input, or its ARC port only with --cec-arc) <--micro-HDMI-- hub HDMI0 (CEC)
 hub --Ethernet-- house LAN -- server (endpoint A) and the room's wired speakers
 for a TV without optical: TV HDMI ARC/eARC --> BK-931 --optical out--> Digi+ I/O optical in
```

Enable the HAT with `dtoverlay=hifiberry-digi` in `/boot/firmware/config.txt` (the overlay name
is `ASSUMED`: confirm it on HiFiBerry's page [H7] before rebooting), reboot, and take the card's
number from `arecord -l` (`hw:<digi card>,0` below). Set the TV's digital output to PCM (P2 [S3]:
"Set the digital audio output on your TV to PCM").

#### S8.1 The capture, the TV's rate and the receiver with the TV off

On the hub, a capture probe with the TV playing, then with the TV in standby:

```
timeout 10 chorus-client --line-in hw:<digi card>,0 --line-in-kind optical --probe-line-in
```

**Expected** with the TV playing: `line-in-probe device=hw:<digi card>,0 usable=1
frames_read=9600 overran=0 delay_frames=<n> signal=1 frame_len=4`. With the TV in standby, one
of two answers, both recorded: the probe returns with `signal=0` (the receiver free-runs and
reads zeros) or `timeout` ends it with exit 124 (the receiver stalls reads). What changes: a
stall is what ADR 0090's `no-lock` refusal (100 ms without frames, `NO_FRAMES_MS`, `ASSUMED`)
and its bounded read (`TV_READ_WAIT_MS` 20) were built for; free-running zeros mean the TV's
standby reaches chorus only through CEC (S8.2) and the level detector's hold.

Then a real session, as the room's hub (the server on endpoint A plays the room; the TV input
autoplays per `docs/control-plane.md` "TV autoplay"), for ten minutes of TV:

```
chorus-client --server <server>:4010 --zone <room> --line-in hw:<digi card>,0 \
    --line-in-kind optical --line-in-name TV --cec /dev/cec0 2>&1 | tee s8-hub.log
```

At the session's end (Ctrl+C) the client prints `source-tv ppm=<p> relocks=<n>
ring_overflows=0 ring_underflows=0 non_pcm_periods=0 refused_non_pcm=0 refused_no_lock=<n>
refused_rate=0 frames_dropped=<n>`. **Expected:** `ppm` inside +/-1000 (IEC 60958-3 Level II,
ADR 0090), no ring over- or underflow, `refused_rate=0`. Repeat on each TV. What changes: the
TV's measured rate error replaces the modelled +/-200 and +/-1400 ppm sources of
`tests/tv_capture.rs` as the case that matters, and a TV near the +/-1500 ppm clamp is named in
the Needs item.

#### S8.2 CEC through chorus's own client (no `cec-ctl` needed)

Give the service the CEC node first (`docs/cec.md` "Bench steps", step 4), with the TV's CEC on
(Roku TV: Settings > System > Control other devices (CEC)). In `s8-hub.log` from S8.1:

- `cec started device=/dev/cec0 ...` then `cec claimed logical_address=5 physical_address=<p.p.p.p>`;
- the TV turning System Audio Mode on: `cec system-audio-mode on=1`;
- the TV's remote, volume up, down and mute: `cec command=volume_step value=2 key=volume-up
  sent=1`, `... value=-2 key=volume-down ...`, `cec command=mute_set value=1 key=mute sent=1`,
  and the room's volume moving on the control page while the TV plays (ADR 0092: the keys no
  longer detach the room from its TV autoplay);
- the TV to standby: `cec tv-power state=standby` and `source-offer source_id=1 signal=0
  reason=standby`, and the room going back to what it played before the TV.

**Expected:** each line in that order; no `cec transmit-failed`; `cec unusable` names why if the
node is missing. If `/dev/cec0` never claims (the Pi's port sees no TV), try `/dev/cec1` (the
other connector), then T4 (`docs/cec.md`, "A Pulse-Eight USB-CEC adapter"). What changes:
whether this TV forwards volume keys to an Audio System on a non-ARC input (a LEAD in the
research, `docs/research/research-tv-path.md` section 1), `--cec-arc` for a TV on its ARC port,
and the `ASSUMED` 30 s power poll if the TV announces nothing.

#### S8.3 The non-audio bit: what the receiver's ALSA controls expose

```
amixer -c <digi card> contents > s8-amixer.txt
amixer -c <digi card> contents | grep -i -E 'iec958|spdif|status|audio'
```

Then switch the TV's digital output to "Auto passthrough" or Dolby Digital (a film with a 5.1
track) and back to PCM, running the second command in each state. **Expected:** a control whose
value changes between the two (an IEC958 channel-status mask, or a "non-audio" switch), or no
such control at all, recorded either way. Also in `s8-hub.log` with the bitstream on:
`source-tv-refused source_id=1 reason=non-pcm` within one period and `refused_non_pcm=` above 0
at the end (the IEC 61937 preamble scan, ADR 0090). What changes: a named control lets
`CaptureSource::channel_status` read the bit (today it reads none, ADR 0090 follow-up); a
`non-pcm` refusal on PCM programme would be a false positive for the 96-bit sync code follow-up.

#### S8.4 The hub's wakeup jitter at the capture's period

The existing host probe (`crates/hostprobe`, ADR 0047; `docs/measurements/host-wakeup-jitter.md`
for the method), at the TV capture's 5 ms period (`PERIOD_FRAMES` 240) and at 1 ms, with the
hub otherwise idle and then during S8.1's session:

```
target/release/chorus-wakeup-probe --period-us 5000 --seconds 600 --raw hub-w5ms.raw
target/release/chorus-wakeup-probe --period-us 1000 --seconds 600 --raw hub-w1ms.raw
```

**Expected:** a lateness table (p50, p99, p99.9, max). What changes: ADR 0090's DLL tests assume
wakeups late by up to 100 us (uniform, `tests/common/tv.rs`); a p99.9 well above that moves
`DLL_BANDWIDTH_HZ` and the ring (`RING_FRAMES`), and the budget's 7 ms capture term (one
period plus the 2 ms send delay, ADR 0094; ADR 0091 had 2 ms).

#### S8.5 The lip-sync measurement and the A/V trim

BRIEF.md section 5.7: "calibrated with a flash+beep clip and a high-frame-rate phone camera". On
the TV, play a flash+beep sync clip (a white frame and a short beep in the same frame; any
published A/V sync test clip, or one made with any video editor) with the room playing the TV
through chorus. Film the TV screen and one of the room's speakers in one shot at 240 frames a
second, from about 1 m, five times. On the computer, step frame by frame: for each flash, the
frame the screen first lights and the frame the beep's waveform starts in the video's audio
track (or the speaker cone moves), each frame 4.17 ms. Sign as ITU-R BT.1359-1 does: positive =
sound leads.

**Expected:** within the window chorus targets, audio -40..+15 ms against video (BRIEF.md section
2.2), at `av_trim_ms` 0; the low-latency path's budget predicts about -21 ms with a
standard-mode TV (`docs/measurements/low-latency-budget-sim.md`, simulation, not timing evidence).
Then set the trim to the measured lead and film again:
`{"v":2,"t":"av_trim","zone":"<room>","av_trim_ms":<ms>}` on the control page's command box
(positive delays the audio). Repeat with the TV's game mode on and off. What changes: the room's
`av_trim_ms` (per TV), the TV's own audio lag in ADR 0091's lip-sync sum (`ASSUMED` 1 ms
standard, 66 ms game mode, one set, LEAD), and whether the -100..+200 ms trim bounds hold.

#### S8.6 The LAN's loss, bursts and jitter for the UDP legs

`chorus-udp-loss` (`crates/hostprobe`, goal 13) sends one 1472-byte datagram every 2.5 ms (the
low-latency wire's chunk at its defaults, ADR 0091) and the receiver tallies loss, the
burst-length histogram, reordering, duplicates and RFC 3550's transit variation. On the hub
(receiving) and on endpoint A (sending), both wired, for 24 hours:

```
target/release/chorus-udp-loss recv --bind 0.0.0.0:47100 --seconds 86460 | tee s8-udp.txt   # on the hub, first
target/release/chorus-udp-loss send --to <hub>:47100 --seconds 86400                      # on endpoint A
```

**Expected:** `udp-loss role=recv ... received=34560000 lost=<n> loss_ratio=<r> ...`, one
`udp-loss burst length=<L> count=<n>` line per burst length and a `transit-variation` table. If
the hub's firewall drops the port, open it for the run only. What changes: ADR 0091's FEC
choice. Single losses only: depth 1 (the default) stands. Bursts of 2 or more: depth 2 with
`L_tv` 35 ms (ADR 0094; 30 ms before the hub's real capture term), which the simulator
already prices. The `transit-variation` p99.9 replaces the 2 ms
jitter margin, and a loss ratio well above 1e-3 per leg is a wiring finding before it is a FEC
one. (One-way delay, the `ASSUMED` 0.25 ms per leg, needs agreeing clocks and is not measured
here.)

#### S8.7 The receiver's output with no TV signal, on the speakers

With the room playing the TV input and the TV switched off by its power button (not standby
through CEC), listen on the room's speakers and on S1's headphones through the UMC202HD for a
minute. **Expected:** silence, no click train or buzz, and `source-tv-refused ... reason=no-lock`
in the hub's log if S8.1 found a stall. What changes: a buzz means the receiver free-runs noise,
not zeros, and the level detector's threshold needs the measured floor.

#### S8.8 The ARC extractor instead of optical (a TV without optical, or to compare)

Rewire through T3 (the wiring's last line), set the TV's ARC on, repeat S8.1 and S8.2.
**Expected:** as optical. Also: does the extractor answer CEC as an Audio System itself? In the
hub's log, `cec claimed logical_address=5` means address 5 was free; a claim of another address,
or the TV sending its volume keys elsewhere (no `cec command=` lines), means the extractor holds
the Audio System role. What changes: whether P2's ARC path works with chorus as the TV's Audio
System, or the room's volume keys need the extractor's own CEC disabled (record its menu or
switch).

#### S8.9 CEC volume with optical, not ARC

With the TV's sound on optical (S8.1's wiring) and the hub's HDMI on an ordinary input (not
ARC), press the TV remote's volume keys. **Expected:** `cec command=volume_step` lines, or none
(the TV sends volume keys to an Audio System only on its ARC port: the research's LEAD for Roku
TVs). What changes: if none, the owner's remote works only with ARC (T3, or the hub on the ARC
port with `--cec-arc`), recorded per TV.

**The reports.** None of S8's steps has a bench script, a grader or a topic in
`tools/bench/topics.conf` (a topic needs its grader); each result is written by hand as a report
under `docs/measurements/` with `Source: hardware`, the build commit, the device notes (TV model
from the Needs item, the hub, the receiver) and the raw files (`s8-hub.log`, `s8-amixer.txt`,
`hub-w5ms.raw`, `hub-w1ms.raw`, `s8-udp.txt`, the lip-sync frame counts) under
`docs/measurements/raw/tv-capture-<date>/`, on a `bench/<date>-tv-capture` branch with a pull
request from the hub, as the production wakeup run does (section 6). A later chorus goal
validates it and replaces each `ASSUMED` value it answers.

### S9. Wi-Fi provisioning from a phone (goal 14; after S5; the compact Wi-Fi profile)

Needs: one ESP32-S3 board that S5 showed reaching Play (a TAS5825M board, the Audio Brick or the
owner's own: the image brings the amplifier up before the link, so a bare development board stops
at `amp=amplifier-did-not-answer` and never reaches provisioning), its USB cable and serial
console, ESP-IDF v6.1 (the pin in `firmware/config/endpoint.conf`), a phone, and the house's
Wi-Fi network on 2.4 GHz with a WPA2 passphrase (the ESP32-S3's radio is 2.4 GHz only: ASSUMED
from the chip family, verify against the datasheet; an open network is refused by design). No
bought part. A chorus server on the network is not needed for this session.

This is goal 14's hardware step for the compact Wi-Fi speakers (K91): the speaker learns its
network at run time from a phone, because the repository declares `link_wifi_ssid` and
`link_wifi_secret` `unknown` and always will. The host build grades every decision
(`make firmware-check`, `test_provision`); the binding to the radio, the speaker's own page and
ESP-IDF's provisioning manager has never run, and this session is its first run (ADR 0103). It
flashes with the guarded tool, run by the owner only:

```
. <esp-idf v6.1>/export.sh
CHORUS_BOARD_PROFILE=compact-s3-wifi make firmware-image     # builds firmware/build/image
CHORUS_BOARD_PROFILE=compact-s3-wifi tools/firmware-flash.sh --print --port /dev/ttyACM0   # shows the esptool command, runs nothing
CHORUS_BOARD_PROFILE=compact-s3-wifi CHORUS_OWNER_AT_BENCH=1 tools/firmware-flash.sh --port /dev/ttyACM0
cd firmware && idf.py -B build/image -p /dev/ttyACM0 monitor
```

Never run `espefuse` or any eFuse command (BRIEF.md section 3.1 rule 2); the image guard in
`make gate` already proves the image burns none. Never type the network's name or passphrase into
a file in the checkout or a command line: they go into the phone only.

**S9.1 First boot.** Expected on the console, after the amplifier's lines:

```
chorus-store: store: NVS ready, namespace chorus
chorus-provision: provision: setup secret created
chorus-provision: provision: unprovisioned
chorus-provision: provision: setup secret <12 characters> (the access point's passphrase and the proof of possession)
chorus-provision: provision: page http://<address>/
chorus-provision: provision: AP up name=chorus-setup-<6 characters>
```

Write the 12 characters down (they stay the same for this board until its flash is erased).

**S9.2 The browser path (the one the web app will walk people through).** On the phone, join the
Wi-Fi network `chorus-setup-<6 characters>` with the setup secret as its password (iOS: Settings,
Wi-Fi; accept "no internet"). Open the address of the `provision: page` line in the phone's
browser. Expected: a page titled "chorus speaker setup" with two fields. Type the house network's
name and its passphrase, press Join. Expected: a page that says "Received", and on the console
within about 30 s (ASSUMED: one join may take up to 20 s):

```
chorus-provision: provision: credentials received ssid_bytes=<n> secret_bytes=<n>
chorus-provision: provision: credentials stored
chorus-provision: provision: AP down
chorus-provision: provision: joined
```

then the link line with `transport=wireless wifi_ps_declared=none wifi_ps_in_force=none`, then
the session's own lines (it reports no server until one is configured or discovered; that is not
this session's concern). The phone drops back to the house network on its own.

**S9.3 A wrong passphrase.** Before S9.2, or after S9.5's reset: post the form once with a
deliberately wrong passphrase. Expected: `provision: join failed reason=auth-error` (or
`reason=network-not-found` for a mistyped name), no `credentials stored` line, the access point
still up; join it again, reload the page, and it shows the same reason above the form.

**S9.4 Reboot.** Power-cycle the speaker. Expected, with no access point at any time:

```
chorus-provision: provision: setup secret loaded
chorus-provision: provision: provisioned at boot
chorus-provision: provision: joining (attempt 1 of 3)
chorus-provision: provision: joined
```

Optional: switch the router's Wi-Fi off and power-cycle the speaker. Expected: three
`join failed reason=network-not-found` lines, then `AP up` under the same name; switch the Wi-Fi
back on and within about two minutes (ASSUMED 120 s) `provision: trying the stored network again`
and `provision: joined`, with nobody touching the speaker.

**S9.5 Reset.** At the console type `wifi-reset`. Expected:
`provision: reset, the stored network is erased` and
`provision: restart the speaker to set it up again`. Power-cycle it. Expected: S9.1's lines again
with `setup secret loaded` in place of `created`, and the same name and secret.

**S9.6 Espressif's phone app as the cross-check (optional).** After S9.5, install Espressif's
"ESP SoftAP Provisioning" app (App Store id1474040630; Play `com.espressif.provsoftap`). In the
app choose to provision without a QR code, join `chorus-setup-<6 characters>` when it asks, enter
the same setup secret as the proof of possession, pick the house network and enter its
passphrase. Expected on the console: the four lines of S9.2. The app's last tick may not arrive
(Espressif documents this for SoftAP: "it may cause connection status updates not to be reliably
received by the phone"); the console is the authority. The app's screens are not something this repository has seen: say what
differed.

Paste into the Needs item: only the console lines that contain `provision:` or `store:`, with the
`setup secret <12 characters>` line deleted, plus any `MAC:` line deleted if one is in the range
(K27); those lines carry no network name, no passphrase and no address of the house's network by
construction, and nothing else from the log is needed. Add the phone's model and browser, the
seconds from pressing Join to `provision: joined`, and for S9.6 whether the app's last tick
arrived.

What changes: goal 14's provisioning line gains its hardware evidence, or a named fault to fix;
the ASSUMED values of ADR 0103 (three joins at boot, the 120 s retry, the 20 s join timeout, the
one-second reply grace, the HTTP server's default header limit against this phone's browser) are
replaced or confirmed; and WIFI-7's characterization run (section 6) has the provisioning path it
was waiting for.

### S10. OTA on the board: identity kept, adopted, a good install, a bad image rolled back (goal 14; after S5; the wired profile)

Needs: the board of S5 and S7 (a TAS5825M board that reached Play; the default `brick-s3-wired`
profile, its W5500 on the audio network), 8 MB of flash or more (S5 says), its USB cable and
serial console, ESP-IDF v6.1, and a chorus server on the audio network that the owner runs with
the bench variable in its environment for this session only (`docs/firmware-updates.md`, "The
bench variable"). No bought part.

This is goal 14's hardware step for OTA (K93): the board keeps its id and key across power
cycles (ADR 0104), is adopted and named, installs a newer image only when the install command is
sent (ADR 0110), confirms it after it rejoins the server, and rolls back a deliberately bad image
through the real bootloader (ADR 0108). The host build grades every decision (`test_ota`, the
`firmware_install` tests) and the emulator ran the same flow (`make ota-qemu`, ADR 0111,
`docs/measurements/ota-qemu-rollback.md`, simulation); this session is the first run on silicon.
Three images are built from one commit, each with its own version, the bad one with the define
that makes it never confirm (`-DCHORUS_OTA_NEVER_CONFIRM`, ADR 0108); the first is flashed over
USB with the guarded tool, the other two travel over the network:

```
. <esp-idf v6.1>/export.sh
make firmware-image                                            # image A, flashed below
tools/firmware-flash.sh --print --port /dev/ttyACM0            # shows the esptool command, runs nothing
CHORUS_OWNER_AT_BENCH=1 tools/firmware-flash.sh --port /dev/ttyACM0
cd firmware && idf.py -B build/image -p /dev/ttyACM0 monitor
```

The good and bad images are built into their own build directories the way
`tools/ota-qemu-run.sh` builds them for the emulator (with the board profile `brick-s3-wired`
in place of the emulator's), then staged on the server with `tools/firmware-stage.sh <build-dir>
<firmware-dir> good` and `... bad` and a `firmware_rescan` (`docs/firmware-updates.md`).

Never run `espefuse` or any eFuse command (BRIEF.md section 3.1 rule 2): the image guard in
`make gate` proves no image burns one, and anti-rollback, Secure Boot and Flash Encryption are
off in every build.

**S10.1 Boot and identity.** Expected on the console: `chorus-ota: running slot=0 state=valid
version=<A's version>`, `chorus-identity: identity id=chorus-<12 hex> key=<fingerprint>
id_made_this_boot=1 key_made_this_boot=1`, then the session reaching the server; on the server
`endpoint adopted id=chorus-<12 hex>`. If instead `the session task could not be started` appears,
paste that line whole: it names the internal RAM free and the largest block (the emulator needed
a larger internal pool, ADR 0109, and whether the board does is this session's question).
Power-cycle the board. Expected: the same id and key with `id_made_this_boot=0
key_made_this_boot=0`, and the server reports the session `verdict=known`, not adopted again.

**S10.2 Name and room.** Send `speaker_name` and `speaker_room` for the id
(`docs/control-plane.md`). Expected: the state's `speakers` lists it named and in the room, and
it plays that room's stream.

**S10.3 Nothing installs by itself.** Stage the good image. Expected: the speaker's
`firmware.update_available` is true, its console shows no transfer, and a power cycle of the
board and a restart of the server change neither.

**S10.4 The good install.** Send `firmware_install` naming the speaker and `good`. Expected:
the state goes `receiving`, `verified`, `pending_verify`, `confirmed`; the console shows
`rebooting into the new image`, then `running slot=1 state=pending_verify version=<good>`,
then, after the session rejoins, the slot confirmed. Note the seconds from the command to
`confirmed`.

**S10.5 The bad image.** Stage the bad image, send `firmware_install` with `bad`. Expected: it
installs and reboots into slot 0 on trial, never confirms, and within the trial window
(`ota_confirm_seconds`, ASSUMED 60 s) `this image did not confirm in time; marking it invalid and
rebooting`; the bootloader starts the good image again (`running slot=1 state=valid
version=<good>`), and the state shows `rolled_back` with reason `not_confirmed`, `image_version`
the bad one and `version` the good one.

Paste into the Needs item: the console lines containing `chorus-ota:`, `chorus-identity:`,
`chorus-discovery:` or `chorus-endpoint:`, the server's `endpoint adopted` and `firmware` lines, and the speaker's
`firmware` object from the state after S10.4 and after S10.5, with any `MAC:` line and any LAN
address deleted (K27), plus the seconds of S10.4. Afterwards remove the bench variable from the
server's environment again.

What changes: goal 14's OTA, explicit-install and adoption lines gain their hardware evidence or
a named fault to fix; the ASSUMED 8 MB layout, the 60 s trial, the chunk window and the board's
internal RAM for the session stack are confirmed or replaced.

### S11. Telemetry from real speakers: one scrape, wired and Wi-Fi (goal 15; after S10, and S9 for the Wi-Fi half)

Needs: the wired board of S10 (adopted, playing), for the Wi-Fi half the board of S9 on the
compact Wi-Fi profile, both flashed (the guarded tool, as in S10) with a build from a commit that
has the exporter's firmware half (ADR 0115), and the chorus server of S10 on the audio network.
`curl` on the bench machine. No bought part.

This is goal 15's hardware step. The server's exporter (`GET /metrics` on the control port,
`docs/telemetry.md`) is graded in the gate against a host endpoint that reports stated fake
values for link, RSSI, heap and temperature (`metrics_scrape`); what the board's own binding
reads (`firmware/main/esp_hal.c`) has compiled in every image and never run on silicon. The
alert rules proposed to the homelab carry floors and windows marked ASSUMED until this session.

```
curl -s http://<server>:<control port>/metrics | grep '^chorus_' > scrape.txt
chorusctl --server <server>:<control port> endpoints list
```

**S11.1 The wired speaker.** While it plays, scrape. Expected for its `speaker` label:
`chorus_speaker_connected` 1, `chorus_speaker_link_info{...,link="wired"}` 1, a sync error, a
buffer fill, a rate correction, `resyncs_total` and `underruns_total`, `heap_free_bytes` and
`heap_min_free_bytes`, `chorus_speaker_firmware_info` with the flashed version, and no
`chorus_speaker_rssi_dbm` and no `chorus_speaker_temperature_celsius` line (no board profile has
a temperature source; a line there is a fault to report).

**S11.2 The Wi-Fi speaker.** Scrape with the speaker near the access point, then carried two
rooms away, a minute apart. Expected: `link="wifi"`, and `chorus_speaker_rssi_dbm` lower (more
negative) in the second scrape.

**S11.3 The heap floor.** Leave the wired speaker playing for an hour, then scrape. Note
`chorus_speaker_heap_min_free_bytes`: it is the floor the homelab's heap alert should sit under.

**S11.4 A speaker that goes away.** Pull the wired speaker's power, wait a minute, scrape.
Expected for its label: exactly `chorus_speaker_connected` 0, `chorus_speaker_info` and
`chorus_speaker_firmware_info`; every other series of it gone.

Paste into the Needs item: the `chorus_speaker_` lines of each scrape (the `speaker` label is the
random `chorus-<12 hex>` id, not a hardware address; delete any LAN address, K27) and
`chorusctl endpoints list`'s output from S11.1.

What changes: `docs/telemetry.md`'s table of what the firmware fills is confirmed or corrected;
the ASSUMED heap floor and Wi-Fi level of the homelab's `chorus` alert rules get measured
replacements; a report under `docs/measurements/` (Source: hardware) records the four scrapes.

### S12. A real control point casts to a room and a group (goal 16; after S1, or S7.1 for a board)

Needs: a chorus server on the house network started with `--upnp` (`docs/upnp.md`, "Turning it
on"; a build from a commit that has ADR 0125), at least two rooms that play (the endpoints of S1
or S7), a phone or computer on the same network with a UPnP AV control point application of the
owner's choice, and a few music files the application can serve (FLAC or MP3; an album whose
tracks run into each other for S12.3). `curl` on the bench machine. No bought part.

This is goal 16's hardware step. The renderers are graded in the gate by a scripted control
point over loopback (`crates/server/tests/upnp_control_point.rs`): no control point application
has been run against them, no discovery datagram of that test crosses a multicast group, and the
join is compared in samples, never heard. What is not known: whether the applications people
use find the renderers, like their descriptions and play through them.

```
ss -lun | grep ':1900 '
curl -s http://<server>:<control port>/api/state
```

**S12.1 Is the discovery port free.** On the host that will run the server, before starting it:
the `ss` line above. Expected: nothing. A line means another program already answers discovery
there; note which, and start the server as `docs/upnp.md`, "A host that already runs an SSDP
program", says.

**S12.2 Discovery and one track.** Open the control point application. Expected: one renderer
per room, per saved group and per live group, each under its room's or group's name. Play one track to one room. Expected: sound in that room; the
`curl` line shows that room's `now_playing` with the track's title and artist and `"via":"upnp"`.

**S12.3 Gapless.** Play the album to the room and listen at two track changes. Expected: no
silence and no click at a change (applications differ: some send the next track ahead, which is
what makes the join gapless; one that does not is worth naming in the answer).

**S12.4 A group takes its rooms.** While room A plays something else, play a track to a saved
group that holds A and B. Expected: both rooms play the group's track together (K78).

**S12.5 The volume limit.** Set a limit on room A (`chorusctl volume limit <room> <limit>`), then
raise the volume to full from the control point application. Expected: the room stays at the
limit and the application's slider shows the limited value after a moment.

Paste into the Needs item: the application's name and version, the `ss` output, what S12.2 to
S12.5 showed, and the server's log lines starting `chorus-server: upnp` (delete any LAN address,
K27).

What changes: `docs/upnp.md` gains a list of control points tried and what each did; an
application that fails to find or play becomes a fix in the goal that reads the answer; the
port answer decides whether the homelab deploy needs `--upnp-ssdp-port`.

## 4. Order and what each session unblocks

S0 first; S1 needs one Pi; S2 needs both and the interface; S3 needs S2's rig trusted; S4 after
S3. S5 is independent (the owner's own board) and can run any time; S6 follows S5 on the same
board. S7 needs an Audio Brick (B1): S7.1-S7.3 after S0 alone, S7.4 after S2's rig is trusted,
S7.5 whenever a logic analyzer and a free pin exist. S8 needs the three TVs' answers first, then
one Pi as the hub (S0) and a receiver HAT (T1 or T2); S8.5 and S8.7 also need S1's endpoint. S9
follows S5 on a board whose amplifier bring-up reaches Play, flashed with the compact Wi-Fi
profile; it needs a phone and the house's 2.4 GHz network and no chorus server, and it unblocks
WIFI-7's characterization run (section 6). S10 follows S5 (and S7.1, so the board is known to
play) on the wired profile with a chorus server on the audio network. S11 follows S10 on the
same board and server (its Wi-Fi half follows S9). S12 needs only a server with `--upnp` and two
rooms that play (S1 or S7.1) and a phone. None
of these blocks a chorus
goal (K7): each goal that can use a result re-checks for its pull request.

## 5. How results come back

Each evidence run (S1-S4, S6, S7.3, S7.4) commits its report on `bench/<date>-<topic>` and opens a pull request
from endpoint A (`docs/bench.md`); the owner does nothing else. If the report or pull-request half
fails after a measurement, nothing is lost: fix the cause and run the same script with
`--report-from <run directory>` (printed at the start) from the same commit. S0, S5, S9, S10 and S11 are
answered in their Needs items; S9's answer is console lines only, chosen so that no network name,
passphrase or address is pasted.

## 6. Not in this packet

- A reader for the GPIO marker's capture (S7.5 records it as a CSV only) and a marker on the
  Linux client: chorus goal 10 decided not to build the Linux marker before a Linux board and pin
  exist and assigned it to goal 26 (ADR 0067), and the goal that has a capture to read writes its
  report.
- WIFI-7 (`tools/wireless-characterization-run.sh`, the compact speakers' Wi-Fi tier): it needs a
  speaker on a wireless network the repository declares unknown. Chorus goal 14 built the
  provisioning path (S9); the run itself still waits for S9's answer and the capture rig.
- A QR code for the speaker's setup network, a captive-portal redirect so the phone opens the
  join page by itself, and a list of scanned networks on the page: none is built (ADR 0103); the
  page is reached by typing the address the console prints.
- The production host's SCHED_FIFO wakeup-jitter run: a homelab-side Needs item, not a bench
  session (`docs/measurements/host-wakeup-jitter.md`). Its output files come back the bench way:
  committed under `docs/measurements/raw/production-wakeup-<date>/` on a
  `bench/<date>-production-wakeup` branch with a pull request, from which a chorus goal writes the
  report (there is no bench script for it yet).
- A bench script and grader for S8 (the TV capture session): its reports are written by hand
  until a goal adds the `tv-capture` topic with its grader.
- `make verify-host` (the host contract and the spin test): they need a real-time priority grant,
  a host setting rather than bench hardware. Since chorus goal 10 the Linux endpoint package
  carries them as `chorus-verify-host` (ADR 0069); running it on an endpoint, under the service's
  own limits, is the owner's (`docs/linux-endpoint.md`, "The host probes").
