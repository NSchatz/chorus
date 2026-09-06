# The verification record

Which criterion each committed check answers, what actually ran when this work
was built, and - for the checks whose environment was not there - the exact
command, the prerequisite that was missing, the verbatim refusal, and where the
instructions are for whoever can run it.

This file is committed inside the repository on purpose. A record of what was
and was not run is only useful to a reader who can read it, and a reader who
picks up this tree later has no access to the notes the implementing session
wrote elsewhere. If the two ever disagree, this file is the one attached to the
code.

Four phases are recorded here, most recent first:

- [EMBEDDED-5, the ESP32-S3 endpoint](#embedded-5-the-esp32-s3-endpoint)
- [SYNC-4, the sync loop on the real path](#sync-4-the-sync-loop-on-the-real-path)
- [RIG-3, the measurement harness](#rig-3-the-measurement-harness)
- [SOUND-2, first sound](#sound-2-first-sound)

## EMBEDDED-5: the ESP32-S3 endpoint

### The machine this was written on

```
kernel                    Linux 6.12.90+deb13.1-amd64 x86_64
toolchain                 rustc 1.98.1, cc (GCC) for the endpoint's C
ESP-IDF                   absent - IDF_PATH is unset and idf.py is not on PATH
ESP32-S3                  none. There is no serial port and no board
amplifier                 none. No TAS5825M, no I2C bus, no loudspeaker
/dev/snd                  absent - no sound card of any kind
capture device            none. There is no audio interface of any kind here
second endpoint           none. There is one machine and it is this container
container                 a disposable Linux container, not a bench
```

### AC-1 AND AC-3 ARE NOT PASSED

**Stated first, and plainly, because they are the two criteria the phase is
named for and because everything below could otherwise be mistaken for them.**

> **AC-1.** WHEN an ESP32-S3 endpoint plays a grouped stream alongside a Linux
> endpoint THE SYSTEM SHALL hold inter-device error within the bound SYNC-4 met.
>
> **AC-3.** WHEN the I2S slot width is 24 bits THE SYSTEM SHALL configure an
> MCLK multiple divisible by three so the bit-clock division stays integral.

**Neither criterion is passed, neither is skipped-green and neither is
satisfied. Nothing in this repository claims either of them.**

AC-1 needs an ESP32-S3 endpoint with a TAS5825M-class amplifier and a real
loudspeaker, a Linux endpoint to play the same grouped stream, an audio
interface with two channels of capture, and the RIG-3 harness. This machine has
none of those and this pipeline has no route to acquire them. The bound AC-1
names is SYNC-4's own first criterion, which is ITSELF blocked on the same
hardware and is recorded below as not passed. **This phase does not claim it and
does not unblock it.**

AC-3 is graded by MEASURING THE PRODUCED SAMPLE RATE and not by reading the
configuration back, which is the roadmap's explicit instruction for this
assertion: "an MCLK multiple not divisible by three makes the rate imprecise
(`espidf-i2s`), which the servo then spends its whole budget fighting while
looking healthy." A configuration read back agrees with itself whatever the
hardware does. What IS discharged here is the half a host can check, which is
AC-11: the committed configuration is 24-bit slots at an MCLK multiple of 384,
divisible by three, and the build refuses when it is not. **That is a rule
enforced, not a rate measured, and it is not AC-3.**

What the repository has instead is the committed entry point, which refuses.
`make verify` runs it, and this is what it printed here, verbatim:

```
--- endpoint-rig-run.sh (exit 3)
    chorus: the ESP32-S3 endpoint beside a Linux endpoint, on the rig
      criterion:       AC-1 and AC-3, the two criteria of this phase that need hardware
      run:             3900s, of which the first 60s is acquisition
      captures:        6 of 30s, one every 640s
      endpoint:        <unset>
      linux endpoint:  <unset>
      local device:    chorus-no-such-device
      capture:         chorus-no-such-capture-device
      i2s:             48000 Hz, 24-bit slots, MCLK x384
    MISSING PREREQUISITE
      criterion:    an ESP32-S3 endpoint playing a grouped stream alongside a Linux endpoint holds inter-device error within the bound SYNC-4 met, and the sample rate the 24-bit I2S configuration actually produces is measured rather than read back
      prerequisite: an ESP32-S3 endpoint with a TAS5825M-class amplifier and a real loudspeaker attached; CHORUS_ESP32S3_PORT names no serial port
      how to get it: flash the endpoint onto an ESP32-S3 board wired to the amplifier per firmware/config/endpoint.conf's pin map, and set CHORUS_ESP32S3_PORT to its serial port
      this check is NOT passed, NOT skipped-green and NOT satisfied.
pass endpoint-rig-run.sh refuses, names both, and claims nothing
```

The hardware it waits on, in full: an ESP32-S3 board with a TAS5825M-class
amplifier wired to it per `firmware/config/endpoint.conf`'s pin map and a real
loudspeaker on the amplifier; the TAS5825M register map, read off TI's
datasheet and written into that same file, which this phase declares `unknown`
and refuses without; a second machine running `chorus-client`; an audio
interface with two channels of capture, with both endpoints' line outputs wired
into it; and a local ALSA playback device that reports a delay. The script
refuses on each of those in turn.

What an operator with the environment runs:

```
CHORUS_ESP32S3_PORT=/dev/ttyACM0 CHORUS_SECOND_ENDPOINT=user@endpoint-b \
CHORUS_CAPTURE_DEVICE=hw:1,0 CHORUS_CLIENT_DEVICE=hw:0,0 \
    ./tools/endpoint-rig-run.sh          # or: make verify-endpoint-rig
```

Instructions: the script's own header, and
`docs/decisions/0015-the-esp32-s3-endpoint.md` for what every constant it
passes in means. The captures it writes ARE the evidence and can be graded
afterwards, on any machine, by someone who did not take them.

### The other three adopted criteria

AC-2, AC-4 and AC-5 are the roadmap phase's own text, adopted verbatim. Each
describes something the endpoint's logic does, so each is gradeable here.

| criterion | what answers it | result |
|---|---|---|
| AC-2, identity and fault confirmed before output is enabled, and high impedance before any I2S clock change | `make firmware-check` (`firmware/tests/test_amp.c`) | Graded on the ORDER, and read off the SIMULATED HARDWARE's own event log rather than the driver's account of itself: `firmware/tests/fake_amp.c` is the part, the output stage and the I2S controller, all appending to one log the driver has no way to reach. In the healthy bring-up the device-id read and the fault read are both before the output enable, and the high-impedance command is before the FIRST clock change. The fake also records, independently of the log's order, whether the stage was in high impedance AT each clock change: "the output stage was in high impedance at every one of the 1 clock changes". The simulated part powers up in an UNKNOWN state and not a safe one, so a sequencer that assumed high impedance rather than commanding it would fail. The gain write is after the fault read and before the enable. **And every LATER clock change, which is what `chorus/amp.h` step 7 promises**: STOPPING a clock is a clock change, so `fake_stop_clock` records one, and each of the four unwind paths is graded on the same log - a fault read during playback (the dangerous one, because the stage is live when it starts), a part that stops answering, a poll against a register the configuration declares unknown, and an output stage that refuses to enable after the clock was applied. On all four the high-impedance command comes before the clock stops and the stage was dead at every clock change. Restoring the old order on any one of them turns this target red |
| AC-4, the server or the network goes away and comes back, and it rejoins and resumes with no human action | `make firmware-check` (`firmware/tests/session-outage.sh`) | Over a REAL loopback socket against a REAL `chorus-server` process killed with SIGKILL and replaced. Not a mock and not a modelled network. Three shapes: an outage SHORTER THAN ONE CHUNK, measured by the endpoint's own monotonic clock at 100 223 556 ns against a 320 000 000 ns chunk, after which audio played on 2 separate connections; an outage OF MINUTES, 130 s of real wall clock, measured by the endpoint at 131 309 115 166 ns, 32 connection attempts across it (more than a couple, and not a busy loop), and audio again on 2 separate connections; and an outage where 6 connect attempts were REFUSED before a new server process answered, so the connect-refused path ran and not only the peer-closed one, with 25 time-sync exchanges completed across the two connections. Nothing touched the endpoint in any of the three but the passage of time |
| AC-5, a fault is surfaced in telemetry rather than played through | `make firmware-check` (`test_amp.c` and `test_telemetry.c`) | Two halves, graded separately because they fail separately. At the pins: a fault read during playback puts the output stage in high impedance and THEN stops the I2S clock, in that order, and the simulated stage records both. In the telemetry: the published line reads `amp=amplifier-reports-fault amp_fault_bits=0x21 audio=stopped-on-amp-fault`, and the test asserts the string `audio=running` does not appear in it. EVERY non-OK amplifier status stops the audio, not only the one called `fault`: all nine are enumerated and checked, including `amplifier-did-not-answer`, because a part that has gone quiet is not a part that is known to be healthy. `docs/decisions/0015` records the choice of stopping over escalating and why |

### This spec's own criteria

| criterion | what answers it | result |
|---|---|---|
| AC-6, the golden vectors, both directions | `make firmware-golden-vectors` | The vectors are DISCOVERED and not declared: the test enumerates `fixtures/protocol/` with `opendir`, the way `test_sync.c` enumerates `fixtures/sync/`, and round trips every `.hex` it finds. All three pairs found (`time_sync`, `audio_chunk`, `stream_end`) encode to the committed `.hex` bytes exactly and decode from them to the fields the committed `.fields` declares. 46 checks, 0 failed. What the enumeration buys is the drift guard: a committed vector pair the C side does not mirror turns this target RED naming the type, and a type the C side mirrors with no committed pair does the same. Both directions are shown going red on a scratch copy of the fixture directory with a fourth pair smuggled in and with a pair removed, plus an unaltered control that stays green. **What that establishes, exactly**: the C side cannot silently fall behind the committed vectors. It does not by itself read `crates/protocol`; the Rust half of the coupling is `crates/protocol/tests/golden_vectors.rs`, which requires every `MessageType::ALL` entry to have a committed pair under the same directory. A fourth Rust type therefore fails on the Rust side until a pair is committed, and fails here the moment one is |
| AC-7, an unrecognised type is skipped and the session stays open | the same target | An unassigned type byte `0x7f` carrying 4 bytes is skipped rather than rejected, the skip steps over exactly 7 bytes using the length prefix, and the committed `time_sync` frame that follows it decodes - so the session stayed open. A payload longer than the fields this decoder knows about is accepted and the excess ignored. Over a real socket the same rule holds: the endpoint counts skips in telemetry. `docs/decisions/0015` records the reading taken, which is the opposite of the Linux client's and is what AC-7 requires by name |
| AC-8, every committed scenario, and the same sample as the Rust implementation | `make firmware-sync-scenarios` | Both halves. Each of the five committed scenarios drives the modelled error below its declared bound by its declared deadline and holds it for the whole rest of the run, checked sample by sample and not from the settle index: `01-wired-quiet` settled at 1000 ms with a peak of 115 995 ns after settling, `02-wired-loaded` at 1000 ms and 159 055 ns, `03-worst-case-skew` at 1000 ms and 343 183 ns, `04-fine-tier-acquisition` at 4400 ms and 998 000 ns, `05-noiseless-control` at 10 ms and exactly 0 ns, all against a 1 000 000 ns bound. And the selection: across all 360 exchanges of the five scenarios the endpoint selected the same sample as the Rust implementation, and every exchange matched the committed cross-check vector EXACTLY - the same round trip, the same raw estimate, the same filtered offset, the same servo tier and the same correction, bit for bit, including `02-wired-loaded` whose exponential jitter goes through `log`. 137 checks, 0 failed |
| AC-9, an entry point that needs hardware refuses visibly | `make verify` | `tools/unrun-checks-are-visibly-unrun.sh` DERIVES its list recursively from `tools/`, and it now finds and checks 12 environment-dependent entry points where it found 10 before this phase. The two new ones are `tools/firmware-image.sh` and `tools/endpoint-rig-run.sh`; both exit non-zero, name the missing prerequisite AND the criterion, and report nothing as passed, skipped-green or satisfied. The endpoint-rig refusal is quoted in full above |
| AC-10, the record says what ran and what did not | this file | the section you are reading. AC-1 and AC-3 are recorded as NOT passed with the verbatim refusal, the exact command, and the hardware they wait on |
| AC-11, a reserved GPIO or a bad MCLK multiple refuses rather than builds | `make firmware-check` | `firmware/Makefile` compiles NOTHING until `chorus-endpoint-config-check` has passed over `firmware/config/endpoint.conf`, so this is a build that refuses and not a test that fails afterwards. Six smuggled configurations were shown making the entry point exit non-zero: an MCLK multiple of 256 at a 24-bit slot width, a pin in the flash and PSRAM range, a pin USB-JTAG reserves, a pin an octal part reserves, a DMA descriptor placed in external RAM, and a requested analog gain above the committed ceiling. Every refusal names the offending value AND quotes the rule: for the MCLK case, ESP-IDF's own "Please set the mclk_multiple to I2S_MCLK_MULTIPLE_384 while using 24 bits data width Otherwise the sample rate might be imprecise since the BCLK division is not a integer". Every multiple in ESP-IDF's own enum is decided one way or the other rather than the rule being tested at one point, and every reserved range is checked at both ends. 72 checks, 0 failed |
| AC-12, a DMA descriptor in external RAM fails the suite naming the placement | `make firmware-safety-scans` | Two ways, because a runtime flag is easy to satisfy and easy to bypass. At configuration time `dma_descriptor_placement = external` is refused, quoting the platform: "DMA transaction descriptors cannot be placed in PSRAM." In the source, the scan refuses ANY external-RAM placement anywhere in the endpoint tree, and both spellings were shown going red on a smuggled instance naming `firmware/src/i2s.c`: a static `EXT_RAM_BSS_ATTR` declaration and a `heap_caps_malloc(64, MALLOC_CAP_SPIRAM)` call |
| AC-13, an eFuse burn or an OTA activation fails the suite naming the file | the same target | The scan starts green - there is no eFuse or OTA code anywhere in this repository - so its value is that it stays green, and the demonstrations are what make that a check rather than a fact. Three were shown going red on a smuggled instance and naming the file: `esp_efuse_write_field_bit` in `firmware/src/monotonic.c`, `esp_ota_set_boot_partition` in `firmware/src/session.c`, and `esp_ota_mark_app_valid_cancel_rollback` in the same file - the last one because an image that confirms itself on boot has disarmed the only recovery there is. `firmware/main`, which is compiled only by ESP-IDF, is scanned too, for the strongest reason there is: it is the one place in the tree where either call would compile |
| AC-14, a gain above the ceiling refuses to enable output | `make firmware-check` | A requested 6.000 dB against the committed 0.000 dB ceiling is refused with `analog-gain-above-ceiling`; output is NOT enabled, the output stage is left in high impedance, no clock is started, and the refusal names all three things the criterion asks for: "a requested analog gain of 6.000 dB is above the ceiling of 0.000 dB declared in firmware/config/endpoint.conf". The refusal happens with ZERO I2C transactions on the bus, which is stronger than the criterion asks: it is provably before any register write. A gain exactly AT the ceiling is inside it, so the committed configuration is bringable |
| AC-15, a part that does not answer or reports a fault leaves the stage dead | the same target | Five conditions, each leaving the output stage in high impedance with no I2S clock started and output never enabled, each reported by name: a NACK (`amplifier-did-not-answer`), a timeout (`amplifier-i2c-timeout`), a bus error (`amplifier-i2c-bus-error`), a wrong device id (`amplifier-identity-mismatch`, and the fault register is not even read once the identity is wrong), and a fault at bring-up (`amplifier-reports-fault`, with the fault bits carried out for telemetry). A sixth, an output stage that refuses to go to high impedance, stops everything before the bus is touched at all. And the case the committed configuration actually produces: every amplifier register is DECLARED UNKNOWN, so the shipped endpoint refuses by name on each of the eight keys in turn with the bus untouched and no clock started |
| AC-16, a settable wall-clock read on the endpoint path fails the suite | `make firmware-safety-scans` | The endpoint has ONE clock reader, `firmware/src/monotonic.c`, and it names `CLOCK_MONOTONIC`; every other unit takes the time it needs as an argument. Two ordinary spellings were shown going red on a smuggled instance and naming the file: `clock_gettime(CLOCK_REALTIME, ...)` in `firmware/src/session.c` and `gettimeofday` in `firmware/src/telemetry.c`. The completeness half is checked too: a first-party unit added under `firmware/src` that the list does not account for turns the scan red naming `firmware/src/smuggled.c`, which closes the obvious hole of moving a clock read one file down. An exclusion with no reason, or an empty one, is refused by the list parser. 42 checks, 0 failed |
| AC-17, the image build refuses without the toolchain | `make firmware-image` | Exits 3 naming both: "the ESP-IDF toolchain at version v5.3; IDF_PATH is unset, so no toolchain is installed here". A DIFFERENT installed version is refused too, because an image built by a toolchain nobody chose looks exactly like an image that was. **No partial image**: the refusal comes before any output directory is created and before any object is compiled, and `firmware/build/` holds no `image/` directory after the run. That ordering is the assertion, not the message |
| AC-18, CI runs the endpoint's regressions as separately named, fail-on-red steps | `.github/workflows/ci.yml` | Four new steps, each named for one regression so a red build says which one broke: "Endpoint golden-vector round trip", "Endpoint sync-core scenario regression", "Endpoint safety scans", and "Endpoint clocking, amplifier, telemetry and the rejoin". Nothing continues on error and nothing in the file is allowed to. The runner has no ESP32-S3, no amplifier, no ESP-IDF and no sound card, which is exactly the environment that makes the refusals in the `make verify` step meaningful |
| AC-19, `make check` and `make verify` are unchanged in outcome | `make check` | Both still pass. `make check` builds the whole workspace and runs `cargo test --workspace`; the only Rust changes this phase made are additive (`crates/sync` gained `run_recorded`, an `ExchangeRecord`, a `crosscheck` module and a vector-generating binary, with `run` delegating to `run_recorded` so there is one loop and not two) and `crates/sync/tests/crosscheck_vectors.rs` asserts `run` and `run_recorded` produce identical results on every committed scenario. `crates/audio-path` found the new module before a human did - the suite went red on `crates/sync/src/crosscheck.rs` being neither listed nor excluded, which is exactly what that check exists for - and `audio-path.conf` now records the exclusion with a reason. `make verify` passes, with its entry-point count moved from 10 to 12 |

### What the endpoint is, in one paragraph

`firmware/` is a C implementation of the protocol core, the sync core, the
amplifier bring-up sequencer, the I2S clock and pin rules, the session
supervisor and the telemetry surface, plus an ESP-IDF component that binds them
to real drivers. Everything except that last part is compiled and graded by a
host C compiler with no ESP-IDF, no device and no privilege. The
`firmware/src/` files are shared byte for byte between the host build and the
image build, so the code that was graded is the code that would run.

### What did NOT run here, and is not claimed

- **AC-1 and AC-3**, above, in full.
- **`firmware/main/app_main.c` and `firmware/main/esp_hal.c`.** They are
  compiled only by ESP-IDF and NOTHING in this repository compiles them. They
  are scanned as text by the safety scans, and that is the only thing said
  about them. Everything they bind is graded; the wiring is not, and the
  criteria that grade the wiring are the two operator ones.
- **Any image.** None was built, none was flashed, and no device was touched.
- **Any eFuse.** There is no eFuse code in this repository and the scan
  enforces that there is none.
- **Any OTA image or image state.** Same, and `chorus#FLEET-10` owns it.
- **SOUND-2's, RIG-3's and SYNC-4's hardware-blocked rows are unchanged by this
  phase and stay blocked on the same missing hardware. This work does not claim
  them.**

### Running the lot, on a machine that has the environment

```
make firmware-check          # everything above that needs no hardware
make firmware-image          # needs ESP-IDF at the declared version
CHORUS_ESP32S3_PORT=/dev/ttyACM0 CHORUS_SECOND_ENDPOINT=user@endpoint-b \
CHORUS_CAPTURE_DEVICE=hw:1,0 CHORUS_CLIENT_DEVICE=hw:0,0 \
    make verify-endpoint-rig # AC-1 and AC-3
```

## SYNC-4: the sync loop on the real path

### The machine this was written on

```
kernel                    Linux 6.12.90+deb13.1-amd64 x86_64
toolchain                 rustc 1.98.1 (48a229cea 2026-09-01)
/dev/snd                  absent - no sound card of any kind
capture device            none. There is no audio interface of any kind here
second endpoint           none. There is one machine and it is this container
loudspeakers              none
ulimit -r (RLIMIT_RTPRIO) 0
container                 a disposable Linux container, not the deploy target
```

### AC-1 IS NOT PASSED

**Stated first, and plainly, because it is the criterion the phase is named
for and because everything below could otherwise be mistaken for it.**

> **AC-1.** WHEN two wired Linux endpoints play one grouped stream for at least
> one hour THE SYSTEM SHALL hold median inter-device error below 0.5 ms as
> measured by the RIG-3 harness and SHALL perform no hard resync after the
> first minute.

**This criterion is NOT passed, NOT skipped-green and NOT satisfied. Nothing in
this repository claims it.** It needs two wired Linux endpoints, an audio
interface with two channels of capture, and real loudspeakers. This machine has
one endpoint, no capture device and no loudspeakers, and this pipeline has no
route to acquire them. RIG-3, the harness AC-1 is measured by, is itself
blocked on exactly that hardware.

What the repository has instead is the committed entry point, which refuses.
`make verify` runs it, and this is what it printed here, verbatim:

```
--- sync-hour-run.sh (exit 3)
    chorus: the hour-long grouped-stream run
      criterion:     AC-1, the one criterion of this phase that needs hardware
      run:           3900s, of which the first 60s is acquisition
      captures:      6 of 30s, one every 640s
      local device:  chorus-no-such-device
      capture:       chorus-no-such-capture-device
      second endpoint: <unset>
    MISSING PREREQUISITE
      criterion:    two wired Linux endpoints playing one grouped stream for at least one hour hold median inter-device error below 0.5 ms as measured by the RIG-3 harness, with no hard resync after the first minute
      prerequisite: a second wired Linux endpoint to play the same grouped stream; CHORUS_SECOND_ENDPOINT names none
      how to get it: set CHORUS_SECOND_ENDPOINT to 'user@host' for a second wired Linux machine with an ALSA output, wire both endpoints' line outputs into one audio interface, and point CHORUS_CAPTURE_DEVICE at it
      this check is NOT passed, NOT skipped-green and NOT satisfied.
pass sync-hour-run.sh refuses, names both, and claims nothing
```

What an operator with the environment runs:

```
CHORUS_SECOND_ENDPOINT=user@endpoint-b \
CHORUS_CAPTURE_DEVICE=hw:1,0 CHORUS_CLIENT_DEVICE=hw:0,0 \
    ./tools/sync-hour-run.sh          # or: make verify-sync-hour
```

Instructions: the script's own header, and
`docs/decisions/0014-the-sync-loop-on-the-real-path.md` for what every constant
it passes in means. The captures and the two delay logs it writes ARE the
evidence and can be graded afterwards, on any machine, by someone who did not
take them. There are SIX captures of 30 s, spread one every 640 s from t = 60 s,
so that none of them is of the acquisition transient AC-1 excludes by name and
so that what is reported is a distribution over the hour rather than one window:

```
cargo run -p chorus-measure --bin chorus-measure -- lag <capture.wav> --label <name>
./target/debug/chorus-delaylog-check <log> --min-graded-seconds 3600 \
    --require-zero-underruns --require-no-rate-change
```

### The other four adopted criteria

AC-2 to AC-5 are the roadmap phase's own text, adopted verbatim. Each of them
describes something the client's logic does, so each is gradeable here against
a modelled device with no sound card.

| criterion | what answers it | result |
|---|---|---|
| AC-2, the offset comes from the minimum-delay sample and not the newest | `cargo test -p chorus-client-linux --test sync_loop` | three exchanges reporting the same true 1 000 000 ns offset, one of them queued at 20 us and the other two at 900 us and 1500 us in one direction. The published round trip is 20 000 ns, the offset is within 20 us of the truth, and the counterfactual is asserted in the same test: the newest sample's own estimate is out by more than 700 us, so a filter that took it would be visibly wrong. Over a modelled run the published round trip settles at 240 to 300 us against a modelled 240 us floor |
| AC-3, the error is the delay the device reports to the DAC | the same target | two runs handed the same audio and the same timestamps, differing only in what the DEVICE says: 48 000 more frames of reported delay moves the error by 1 000 000 000 ns, to within 1 ns. Both runs wrote 1920 frames and had every frame accepted, so an error derived from the return of the write call could not tell them apart. Structurally: `crates/client-linux/src/sync.rs` contains no `SinkWrite`, `frames_written`, `underran` or `.write(` outside its documentation, asserted by reading the source |
| AC-4, the bound is half the round trip of the sample the offset came from | `--test sync_telemetry` | round trips of 3 ms, 400 us and 1.2 ms in a window; the published round trip is 400 000 ns and the published bound is exactly 200 000 ns. A newer, worse exchange does not become the bound by being newer; a better one does. Over a real client session on a loopback socket, `bound_ns == round_trip_ns / 2` and both are in the delay log |
| AC-5, past the threshold it mutes, realigns and resumes | `--test sync_loop` | a 40 ms error against a 2 ms threshold returns a step and not a slew, the correction in force goes to zero, the splice is muted for the configured 20 ms, the playout pointer moves by the whole step to within 25 us, and output resumes when the mute runs out. The mute silences frames in place and inserts none, so it costs nothing in alignment. Two unit tests hold the boundary the first cut of this work got wrong: a BACKWARD step of twice the mute silences 960 frames of the audio that resumes after the inserted silence rather than 960 frames of that silence, and a chunk with no audio left after the splice carries the mute forward instead of spending it |

### This spec's own criteria

| criterion | what answers it | result |
|---|---|---|
| AC-6, the rig run refuses visibly | `make verify` | `tools/sync-hour-run.sh` exits 3 naming the criterion and the prerequisite, quoted in full above. `tools/unrun-checks-are-visibly-unrun.sh` derives its list recursively from `tools/`, finds 10 environment-dependent entry points and checks all 10 |
| AC-7, the record says what ran and what did not | this file | the section you are reading. AC-1 is recorded as NOT passed with the verbatim refusal, the exact command, and the missing prerequisite |
| AC-8, two clients, one timeline, contiguous runs | `cargo test -p chorus-server --test grouped_stream` | two clients attached before the first chunk is cut receive at least 120 chunks each, share at least 100 sequence numbers, and for every shared sequence the presentation timestamp and the audio bytes are identical. Each client's sequence numbers are a contiguous run. Timestamps are exactly one chunk duration apart, start to start, and the stream's origin is the server's own monotonic reading rather than zero. A client that joins late starts partway into the run, not at sequence 0, and is on the same timestamps for the content it shares. The fanout that makes this possible has no back pressure and therefore has a ceiling: `cargo test -p chorus-server --lib` asserts that a subscriber which stops draining holds exactly `SUBSCRIBER_QUEUE_LIMIT` items and not one more, that the dropped items are counted, and that the client beside it never misses one. **Contiguity is claimed for a client that is draining, which is the condition the test asserts it under and the condition an endpoint playing audio is in.** A subscriber that has stopped draining for 2.56 s has items dropped for it alone, and a gap in the sequence it receives is what that looks like from its end; the alternative was an unbounded queue, the trade is deliberate, and `Fanout::dropped` on the `stream done` line is where it shows. A client arriving when all `max_clients` slots are busy is refused by name and closed, not served with gaps: `cargo test -p chorus-server --test thread_shape` asserts the refusal names the reason and the ceiling |
| AC-9, the exchange is answered on the audio connection | the same target | six requests sent up the connection the audio comes down, six replies, and at least 150 chunks received in the same window with their sequence run unbroken. Each reply echoes the client's own `t0` untouched, carries a nonzero `t1` and a `t2` at or after it, both from the server's `MonotonicTimeline` and both inside the run, never going backwards between exchanges. `t3` is zero on the wire, because it is the client's receive stamp on the client's clock and the server will not invent it; the client stamps it and the completed exchange is four monotonic-nanosecond timestamps. Two clients exchange independently on their own connections against the same server timeline |
| AC-10, the modelled hour | `--test sync_loop` | see below |
| AC-11, every constant with what it was chosen from | `docs/decisions/0014-the-sync-loop-on-the-real-path.md` | the filter window, the smoothing weight, the exchange cadence, the hard-resync threshold, the correction clamp, the staleness limit, the round-trip ceiling, the playout latency and the mute, each with its provenance, and the three inherited values named as inherited. `--test sync_loop` asserts every number in `config/sync.conf` equals the compiled constant of the same name |
| AC-12, the audio-path scan accounts for every new unit | `cargo test -p chorus-audio-path` | `crates/client-linux/src/sync.rs`, `crates/server/src/stream.rs`, `crates/sync/src/lib.rs`, `crates/sync/src/servo.rs` and `crates/sync/src/config.rs` are listed on the path; the simulator's four units are excluded with reasons. All 7 audio-path tests pass, including both red demonstrations, and the check went red first on the unlisted `sync.rs`, which is how the enrolment was found rather than remembered |
| AC-13, a nonsensical exchange is discarded | `--test sync_loop` | three shapes that cannot be a round trip - the server holding a request longer than the whole exchange took, a round trip above the configured ceiling, and a reply that arrived before it was sent - are each discarded by name, the accepted count does not move, and the offset, round trip and bound are exactly what they were before. Over a modelled run with every exchange corrupted from minute two, more than 60 are discarded, not one is admitted, and the underrun count stays at zero |
| AC-14, no exchange means no offset and no bound | `--test sync_telemetry` | `offset_ns`, `round_trip_ns` and `bound_ns` are all absent and the published line reads `offset_ns=none round_trip_ns=none bound_ns=none`; the test asserts the string `bound_ns=0` does not appear. A client whose every exchange was discarded is in the same state and answers the same way. A real client session against a server that answers nothing publishes `none` for all three, inserts zero frames, drops zero frames, applies no correction, writes no `correction` event to its log, and still plays audio |
| AC-15, a silent server leaves audio playing and the offset stale | `--test sync_loop` | after the server stops answering, the loop keeps correcting until the newest accepted sample is older than the 10 s staleness limit, then reports stale and runs the servo not once over the following three modelled minutes. The correction in force is held rather than reset; the offset is still published, with a stale flag on it rather than absent; the underrun count stays at zero and the run keeps producing playout samples. A server that comes back is not a special case: the offset goes fresh again and the servo resumes. **What the stale flag is decided from is the age of the newest accepted sample and nothing else.** A device that answers every delay query with zero - which the ALSA `null` device does for ever, and which a real card can report in an xrun - is corrected against not at all and does not make an old offset read as fresh: `a_device_that_reports_no_delay_does_not_make_a_stale_offset_read_as_fresh` runs six quiet modelled minutes on such a device, where no tick computed a correction and no tick returned the ordinary stale answer either, and the published line still reads `stale=1` with the offset still on it. `cargo test -p chorus-client-linux --test regress_0031_F9` is the reviewer's own artifact on the same point, and `--test sync_telemetry` puts it through the shipped session over a real socket: the log carries `sync-no-device-delay`, a `sync-stale` event and published lines reading `stale=1`, and not one `correction` event |
| AC-16, a device that refuses its delay stops the run | `--test sync_loop` | the loop returns a refusal naming `hw:CARD=sulky,DEV=0`, quoting what the device said, and saying that the return of a write call is not a substitute. The servo was never run and nothing was corrected, even though that device would have accepted every frame handed to it. In the client session this is `StopReason::DelayRefused`, which reports `reason=delay-refused` and exits 4 |
| AC-17, a clamped correction is applied clamped and reported | `--test sync_loop` | a 1 ms error asks for far more than the 300 ppm clamp; the correction applied is exactly -300 ppm, `clamped` is true, and the published line reads `clamped=1`. The clamped value reaches the frames: a second of it at 48 kHz inserts 14 whole frames of silence and carries the remaining 0.4 of a frame. A correction inside the clamp is not reported as clamped |

### AC-10 in full: the modelled hour, and what a model is not

**This is a MODELLED result. It is not a measurement and it is not AC-1.**

`crates/client-linux/tests/common/mod.rs` builds a modelled endpoint: two
modelled crystals, a modelled network drawing from the jitter distributions
`crates/sync/src/jitter.rs` already models, and a modelled DAC draining on a
virtual clock. The `SyncLoop` and `PlayoutCorrector` inside it are the same
types `crates/client-linux/src/run.rs` uses on the real path; what is modelled
is everything around them. Ground truth - how far the content about to become
audible is from where the server timeline says it should be - is computed
outside the loop from state no participant in the model can see.

Over 60 modelled minutes, peak absolute error after the first modelled minute:

| modelled scenario | ground truth | the loop's own error | hard resyncs | after minute 1 | underruns | exchanges |
|---|---|---|---|---|---|---|
| quiet wired, 0 and +40 ppm, uniform 60 us | 69.8 us | 42.0 us | 1 | 0 | 0 | 7200 accepted, 0 discarded |
| wired loaded, -12.5 and +38 ppm, exponential 150 us | 184.4 us | 80.3 us | 1 | 0 | 0 | 7200 accepted, 0 discarded |
| worst crystal pair, +50 and -50 ppm, exponential 150 us | 158.7 us | 90.7 us | 1 | 0 | 0 | 7129 accepted, 0 discarded |

All three hold below the 0.25 ms bound, and the one hard resync in each is the
acquisition in the first modelled minute, which the run is deliberately started
outside the bound to produce. Every figure in every row of this table prints
from `cargo test -p chorus-client-linux --test sync_loop -- --nocapture`, on a
line naming its scenario, so a reader who does not trust the digits can produce
them rather than take them. The counterfactual is asserted in the same test:
the same run with the slew switched off and only the step tier left walks past
four times the bound and keeps stepping, more than five times after the first
minute, which is exactly what the second half of the criterion forbids.

**What this does not say.** That two loudspeakers in a room are inaudibly
apart. That `libasound` reports a delay the way this model does. That a real
crystal drifts like a straight line. Every one of those needs the hardware AC-1
needs, and this record does not pretend otherwise.

### What the ALSA `null` device did establish, and what it cannot

`make verify-null-device` runs the REAL server and the REAL client binaries
against the ALSA `null` device, which opens and accepts frames and reports a
delay of zero for ever. Both runs completed and both published timing health,
so the exchange itself is exercised end to end through the shipped binaries and
not only in a test harness:

```
chorus-client: sync offset_ns=998498213 round_trip_ns=189182 bound_ns=94591 stale=0 \
    age_ns=896560781 correction_ppm=0.000 clamped=0 hard_resyncs=0 accepted=3 discarded=0
```

Three exchanges answered by the server on the connection carrying the audio, an
offset published, and a bound that is exactly half the published round trip.

**What it cannot establish is anything about the correction**, and the run says
so rather than pretending: `correction_ppm=0.000` and zero hard resyncs,
because a device reporting a delay of zero is not reporting a distance to a
DAC, and the loop answers that with `Correction::NoDeviceDelay` and corrects
nothing. `docs/decisions/0014` records why that is a refusal and not a
fallback. The delay log carries a `sync-no-device-delay` event naming the
device. Everything about the correction on a device that paces still needs the
hardware AC-1 needs.

### What an adversarial review of this work changed, and what it could not

The first cut of this phase was reviewed against the spec by a reader who had
not written it. Five things came back; four are fixed in the tree and each is
recorded above where it belongs, and the fifth needs the hardware AC-1 needs.

- **`--serve-forever` had become a silent no-op.** The flag is documented in
  `ServerConfig`, `deploy/run-server.sh` and `deploy/Dockerfile` both pass it,
  and the fanout server had stopped reading `config.once`: the process served
  one stream and exited whatever the command line said. It is honoured again,
  by restarting the producer rather than the accept loop, and
  `crates/server/tests/regress_0031_f1.rs` runs the REAL binary with the flag
  and a finite source and asks a second client for a second stream. No file
  under `deploy/` was changed, which was the alternative.
- **The fanout's per-subscriber queue was unbounded.** Bounded now, with the
  drops counted and reported. See AC-8 above.
- **A hard resync could mute the silence it had just inserted.** Fixed, with
  the boundary asserted. See AC-5 above.
- **The rig run captured 30 s at t = 0.** It captures six times across the
  graded hour now, starting after the acquisition transient. See AC-1 above.
  **This change is NOT demonstrated here.** It is a change to a script that
  refuses on this machine for want of two endpoints and an interface, so what
  ran is the refusal and the arithmetic: `make verify` still finds and refuses
  it, and `cargo test -p chorus-client-linux --test sync_loop` asserts the
  schedule fits inside the run. Whether six windows of a real capture say what
  this record expects is exactly the question AC-1 is, and it is still open.
- **Two of `docs/decisions/0014`'s modelled tables were prose.** Both are
  committed tests now, named in that record beside the rows they produce. The
  `wired loaded` row changed from 125.3 us to 184.4 us in the process, because
  the committed reproduction's seed and initial misalignment are not the ones
  the uncommitted original run used. The number in the record is now the number
  a reader gets.

### What a second review changed: the shape of the process on the deploy host

The same work was reviewed a second time, against the tree the first review's
fixes produced. One finding was blocking and it is about a machine rather than
about audio, so it is recorded here in its own words rather than folded into a
criterion row.

**The scheduling report was being taken while the process was still one
thread.** Before this phase that was the whole truth: the server accepted one
connection on its main thread and spawned nothing, so a report taken at startup
described every thread the process would ever run. Serving two clients from one
stream changed the process's shape and the report did not move with it. The
first cut ran an acceptor, a producer and two threads per attached client, all
created by the thread that had taken `SCHED_FIFO`, and `std::thread::spawn`
inherits the creating thread's scheduling policy: on the Proxmox host, where
`deploy/run-server.sh` passes `--ulimit rtprio=20`, that is six real-time
threads with one client attached, five of which the report the host contract is
graded on had never seen. `EXIT_UNDECLARED_THREAD` and `tools/host-contract.sh`
both grade "no thread is real-time without being reported", and neither can see
a thread that did not exist when the report was taken, so the check answered
clean for exactly the reason `crates/hostctl` calls worse than no answer.

What the process runs now is fixed and declared before the socket is bound:
**3 + 2N threads**, eleven of them at the default `--max-clients 4`. One
supervisor, one audio thread, one acceptor, and two per client slot. The audio
thread is the only one that asks the host for a real-time policy, it takes it
for itself rather than inheriting it, and it creates no thread; the supervisor,
which creates all the others, never holds one. No thread is created after the
report, however many clients come and go, so one report describes the whole run,
and a client past the ceiling is refused by name rather than served by a thread
nobody declared. `docs/decisions/0014` records the shape and why
`max_clients = 4`.

| what ran | what it establishes |
|---|---|
| `cargo test -p chorus-server --test regress_0031_f6` | the reviewer's own artifact, carried into the tree unchanged in its body. Two halves: that a spawned thread inherits its creator's scheduling policy, demonstrated here with `SCHED_BATCH`, which needs no privilege; and that the count the real binary's report published equals `/proc/<pid>/task` while it serves a client. It failed on the tree this finding was raised against and passes now |
| `cargo test -p chorus-server --test thread_shape` | the tids the report names are exactly the tids the kernel lists, no row says `role=unregistered`, the population is 3 + 2N, the audio role is NOT on the main thread (whose tid is the process id, which is what makes this checkable with no privilege), and a client past the ceiling is refused with `reason=no-free-client-slot` without a thread being created |
| `cargo test -p chorus-server --lib` | the pool creates every thread it will ever use before it serves anyone, each registers itself, none asks to be real-time, a slot comes back when its client goes away and serves the next one, and the writer gives its thread back when it is no longer wanted without dropping what is already queued for it |
| `make verify-null-device` | the real binaries: `scheduling-report threads=11 vanished=0 real_time_declared=0 undeclared_real_time=0`, with a row per thread naming its role, on a run that then serves a client and ends its stream cleanly |

**What could NOT be checked here, and is not claimed: the policies themselves.**
This container's granted rtprio ceiling is zero, so every run above is
`--allow-non-realtime` and no thread in any of them is real-time. What is shown
here is that the report accounts for every thread and that the real-time role is
not on the thread that creates the others, which are properties of the code.
Whether the kernel agrees that exactly one thread is `SCHED_FIFO` on a host that
grants a ceiling is `tools/host-contract.sh`'s question; it refuses by name on
this machine, and it grades the granted case wherever one exists.

Two smaller findings came with it. One new delay-log event kind,
`sync-no-device-delay`, was missing from the list of kinds in
`crates/client-linux/src/delaylog.rs` although the format carried it and this
record named it; the list carries it now. And the qualifier AC-8's contiguity
needs, since the fanout gained its ceiling, is stated in the AC-8 row above
rather than left to be inferred.

### What a third review changed: staleness is about the sample, not the device

A third reading of the same work, against the tree the second review's fixes
produced, found one blocking thing and one smaller one.

**A silent server was not reported stale when the device reported a delay of
zero.** `SyncLoop::observe` read the device first and answered a zero with
`Correction::NoDeviceDelay`, returning before it looked at how old the newest
accepted sample was, so the flag `Telemetry::stale` publishes was never set on
that path. The published line then said `stale=0` beside an `age_ns` of seventy
seconds against a ten-second limit: two fields of one line disagreeing about one
fact, and the one a consumer keys on was the wrong one. It is not a state that
has to be contrived - the ALSA `null` device reports zero for ever, which is
what `make verify-null-device` runs the shipped binaries against, and a real
card can report a negative delay in an xrun, which took the same branch.

The age of the newest accepted sample is now what decides staleness, in every
state and whichever way a tick was answered. Nothing about what is corrected
moved: a zero delay is still a reason to correct nothing, and a stale offset is
still holdover rather than a jump to zero. What changed is that the flag can no
longer disagree with the age published beside it.

| what ran | what it establishes |
|---|---|
| `cargo test -p chorus-client-linux --test regress_0031_F9` | the reviewer's own artifact, carried into the tree unchanged in its body. Two halves of one scenario, differing only in what the device says about its delay: the control half with a ring goes stale, and the half on a device reporting zero now goes stale too. It failed on the tree this finding was raised against and passes now |
| `--test sync_loop` | `a_device_that_reports_no_delay_does_not_make_a_stale_offset_read_as_fresh`: six quiet modelled minutes on a device reporting zero. Not one tick returned the ordinary stale answer, the servo was not run once and the correction in force did not move, the device went on playing, and the published telemetry reads `stale=1` with the offset still on it |
| `--test sync_telemetry` | `a_real_client_whose_device_reports_no_delay_still_publishes_a_stale_offset`: the shipped `run_session` over a real loopback socket against a scripted server, on a device reporting zero and a staleness limit shortened to 200 ms so a four-second session can outlive it. The log carries `sync-no-device-delay`, published `stale=1` lines and a `sync-stale` event, no `correction` event, and the run inserted and dropped no frames |

The smaller finding was in `docs/decisions/0014`: its three-scenario
modelled-hour table said every row was a committed test whose figures print, and
one row of the three printed. The two tests that asserted a bound without
reporting the figure now report it, so the claim is true rather than narrowed;
no number in that table changed, and the figures that now print are the figures
that were already recorded.

### What did NOT run here, and is not claimed

Beyond AC-1 above, nothing new. SOUND-2's and RIG-3's hardware-blocked rows are
unchanged by this phase and stay blocked on the same missing hardware; this
work does not claim them. `tools/device-loss-run.sh` gained one accepted stop
reason (`delay-refused`, alongside the two it already accepted) because a
device that has gone away now refuses the first thing the playout loop asks it,
which is how far it is from its DAC. It still needs a device that can be
removed mid-run and still refuses by name here.

### Running the lot, on a machine that has the environment

```
make test                    # the suite: no device, no privilege
make verify                  # every refusal path, including this phase's
make verify-null-device      # the device checks that do not grade the delay
make verify-device           # everything that needs a device that paces
make verify-host             # the scheduling contract and the spin test
make verify-measure-device   # the capture run: needs an interface
make verify-sync-hour        # AC-1: needs a SECOND endpoint as well
```

## RIG-3: the measurement harness

### The machine this was written on

```
kernel                    Linux 6.12.90+deb13.1-amd64 x86_64
toolchain                 rustc 1.98.1 (48a229cea 2026-09-01)
/dev/snd                  absent - no sound card of any kind
libasound.so.2            present, and the ALSA `null` PCM opens for playback
capture device            none. There is no audio interface of any kind here
ulimit -r (RLIMIT_RTPRIO) 0
container                 a disposable Linux container, not the deploy target
```

**This machine has no capture device, so no capture was ever taken here.** Every
figure below is from a committed synthetic capture whose ground truth is stated
in the `.params` file beside it. That is the whole design of this phase, not a
compromise in it: the roadmap phase wants a claim someone else can check, and an
analysis that could only run against a live device could not be checked by
anyone without two endpoints and an interface.

### What ran here

| criterion | what answers it | result |
|---|---|---|
| AC-3 | `cargo test -p chorus-measure --test report_shape` | a completed run writes exactly one new file under `docs/measurements/`; the two committed reports are there, each naming a 40-character commit, and the lag report cites the baseline the free-run report established |
| AC-4 | `cargo test -p chorus-measure --test lag_resolution` | against `fixtures/measure/01-chirp-pair-a.wav`, whose parameters declare 254.0 us (24.384 samples at 96 kHz): median +253.981 us, p95 254.026 us, maximum 254.039 us. Largest error 0.039 us against a 10 us budget, over 32 of 32 windows |
| AC-5 | the same target | fixtures 01 and 02 are 10.0 us apart, which is 0.96 of a capture sample; their medians differ, and differ by 10.0 us to within 1 us. A whole-sample estimator reports one number for both |
| AC-6 | the same target | 01 reports +253.981 us and "channel A leads channel B"; 03, which is 01 with the channels exchanged and nothing else changed, reports the same magnitude negated. The convention is declared in `chorus_measure::lag::SIGN_CONVENTION` and printed in every report |
| AC-7 | `cargo test -p chorus-measure --test free_run_slope` | the noiseless series fits +37.5000 ppm against a declared 37.5 ppm with zero residual; the jittered one, at the 20 us RMS its fixture states and which the residuals measure back to within 15%, fits inside the 5 ppm budget |
| AC-8 | `--test report_shape` | `docs/measurements/free-run-baseline.conf` carries the measured slope, the report that established it and the commit that report was written against; the committed lag report names both the value and that report. `source = fixture` is carried through, so nobody reads it as a statement about a crystal |
| AC-9 | `--test report_shape` | one new file, carrying the commit, whether the tree was clean, 96000 Hz, "32 offered, 32 used", and all three lag figures. Both states are in the committed reports: the free-run one was written from a clean tree and says so, and the lag one, written second, names the two uncommitted paths the free-run one had just created. A dirty tree is asserted separately so a report that always said "clean" would fail |
| AC-10 | `--test report_shape` | all 13 committed fixture inputs regenerate from their committed parameters byte for byte, and a second analysis over the same capture returns an identical `LagSummary` and an identical figures table |
| AC-11 | `cargo test -p chorus-measure --test no_settable_wall_clock` | every one of the crate's 11 units is accounted for in `audio-path.conf`; none of the listed ones reads a settable clock; a read smuggled into `freerun.rs` or `lag.rs` turns the scan red and names the file; and the report writer is the ONLY unit of the crate that reads one, which the exclusion cannot assert about itself |
| AC-12 | `make check` | `cargo build --workspace --all-targets` then `cargo test --workspace`, both clean, on this machine with no audio device and no privilege. 40 test binaries, 0 failures |
| AC-13 | `cargo test --workspace --offline` | passes. Every dependency in the workspace is a `path` dependency on another crate in it; `Cargo.lock` is still untracked and still ignored, and `docs/decisions/0002` is unchanged |
| AC-14 | `--test refusals`, and `make verify` through the real binary | silence, full-band noise and the reversed-chirp pair hit three DIFFERENT named conditions (`silent-channel`, `no-chirp-present`, `below-confidence-floor`), each exits non-zero, each names its condition, none reports a lag, and `docs/measurements/` holds the same 5 files before and after |
| AC-15 | `--test refusals`, and `make verify` | `--amplitude 0.9` exits 4 with "requested: 0.9 full scale", "permitted: 0.25 full scale, declared in config/measure.conf" and "emitted: nothing. No device has been opened." It refuses BEFORE the device probe, asserted by the absence of a probe line. Exactly the ceiling is permitted; a hair over it, and NaN, are not |
| AC-16 | `--test refusals`, and `make verify` | a mono capture ("1 channel(s) and 2 were required"), a float one ("WAVE_FORMAT_IEEE_FLOAT at 32 bits ... WAVE_FORMAT_PCM at 16 bits was required"), a truncated one ("declares 7680 bytes and 3956 bytes are present"), and a rate the run did not declare. Each is a typed error at start and none writes a report |
| AC-17 | `--test free_run_slope` | the 21-observation series refuses as `series-too-short` naming the 30 it required; the 3 ms-jitter series refuses as `series-too-noisy` and says what it withheld. With the bound removed the same series publishes a number wrong by more than the bound allowed, which is what the bound is for |
| AC-18 | `--test refusals` | a missing destination, a destination that is a file, and a destination under a file are each refused naming the path and the reason. The write is a temporary file plus a rename, so a refused write leaves nothing behind at all |
| AC-19 | `make verify` | `tools/measure/capture-run.sh` exits 3 naming the prerequisite and the criterion; `tools/unrun-checks-are-visibly-unrun.sh` now derives its list recursively from `tools/`, finds 9 environment-dependent entry points and checks all 9. See the refusal quoted below |

### AC-19 in full: the device-backed entry point, refusing here

This is the row that matters most on a machine with no interface, so it is
quoted rather than summarised. `make verify` runs it, and this is what it
printed here:

```
--- capture-run.sh (exit 3)
    chorus: the device-backed measurement run
    MISSING PREREQUISITE
      criterion:    two endpoint line outputs captured together, cross-correlated into median, p95 and maximum inter-device lag at 10 us or better
      prerequisite: an ALSA capture device that opens two channels at the declared rate; 'chorus-no-such-capture-device' does not (capture-device-probe device=chorus-no-such-capture-device usable=0 detail=snd_pcm_open on audio device 'chorus-no-such-capture-device' failed: No such file or directory (errno 2))
      how to get it: connect both endpoints' line outputs to the L and R inputs of one audio interface and point CHORUS_CAPTURE_DEVICE at it
      this check is NOT passed, NOT skipped-green and NOT satisfied.
pass capture-run.sh refuses, names both, and claims nothing
```

The same shape every other environment-dependent entry point in this repository
uses, for the reason `tools/lib.sh` gives at the top of itself. A capture run
that reported green on a machine with no capture device would make every row
above worthless.

### What did NOT run here, and is not claimed

#### AC-1 and AC-2: `[grade: operator]`

These two are the roadmap phase's own criteria, adopted verbatim, and in that
form each describes a run against physical hardware:

- **AC-1.** WHEN the harness captures two endpoint line outputs playing a shared
  chirp THE SYSTEM SHALL report median, p95 and maximum inter-device lag at a
  resolution of 10 us or better.
- **AC-2.** WHEN two clients run with correction disabled THE SYSTEM SHALL
  report the slope of their relative offset in ppm and SHALL record it as the
  free-run baseline later runs are compared against.

**Neither is passed here and neither is claimed.** They need two endpoints, an
audio interface and real loudspeakers; this machine has none of those and this
pipeline has no route to acquire them. AC-4 through AC-9 discharge every
OBSERVABLE inside them against committed fixtures with known ground truth, which
is what an operator can then point at hardware - but a fixture is not a
loudspeaker and this record will not pretend otherwise.

What an operator with the environment runs:

```
CHORUS_CAPTURE_DEVICE=hw:1,0 CHORUS_CLIENT_DEVICE=hw:0,0 \
    ./tools/measure/capture-run.sh 30          # or: make verify-measure-device
```

Instructions: the script's own header, and
`docs/decisions/0013-the-measurement-rig.md` for what every declared threshold
means. The capture it writes IS the evidence and can be analysed afterwards, on
any machine, by someone who did not take it:

```
cargo run -p chorus-measure --bin chorus-measure -- lag <capture.wav> --label <name>
```

For AC-2, the free-run half, an operator runs two clients with correction
disabled, logs their relative offset, and fits it:

```
cargo run -p chorus-measure --bin chorus-measure -- free-run <series.offsets> \
    --label <name> --source hardware
```

`--source hardware` is the load-bearing word. The committed baseline in
`docs/measurements/free-run-baseline.conf` says `source = fixture`, and both the
artifact and every report that cites it say in as many words that a
fixture-derived baseline bounds the ESTIMATOR and is not a statement about any
real crystal. The day a hardware baseline is recorded, that word changes and the
sentence goes away.

#### What a committed fixture cannot establish

Stated plainly because the numbers above look precise enough to be mistaken for
something they are not. An error of 0.039 us against a synthetic capture says
the estimator is not what spends the 10 us budget. It says nothing about:

- what two amplifiers, two loudspeakers and a room spend of it;
- whether a real capture interface's two channels are sample-aligned with each
  other, which is an assumption every figure here rests on;
- what a real endpoint's line output actually does at the chirp's band edges;
- the drift of any real crystal, which is AC-2 and is operator graded.

### Running the lot, on a machine that has the environment

```
make test                    # the suite: no device, no privilege
make verify                  # every refusal path, including the rig's
make measure-fixtures        # regenerate the committed fixture inputs
make measure-fixture-reports # the saved reports over those fixtures
make verify-measure-device   # the capture run: needs two endpoints and an interface
```

## SOUND-2: first sound

### The machine this was written on

```
/dev/snd                  absent - no sound card of any kind
libasound.so.2            present, and the ALSA `null` PCM opens
ulimit -r (RLIMIT_RTPRIO) 0
ulimit -l (RLIMIT_MEMLOCK) 8 MiB, below the 64 MiB the server asks for
```

So: everything that needs no device and no privilege ran; everything that needs
only a device that OPENS ran against `null`; nothing that needs a device that
paces, a real-time ceiling, or a device that can be pulled out mid-run ran, and
none of it is claimed.

### What ran here

| criterion | what answers it | result |
|---|---|---|
| AC-1 | `cargo test -p chorus-audio` (chunker), and over a real TCP socket `-p chorus-server --test serving` | ten chunks plus 100 frames plus three bytes gives eleven chunks, only the last short and marked final, concatenation byte-identical to the input |
| AC-2 | the same capture | the sequence column is a contiguous run |
| AC-3 | the same capture | strictly increasing, constant 20 ms start-to-start delta including across the short final chunk |
| AC-4 | `cargo test -p chorus-protocol` | the two FOUNDATION-1 vectors are untouched; the new type has its own committed vector; the live capture decodes with the committed decoder |
| AC-5 | `tools/start-fill-and-log-shape.sh` on the ALSA `null` device, through the real binaries | the first write carried 120000 us of a configured 120000 us fill; the log names it; no sample before it is graded |
| AC-7 | the same run | 594 samples in 60 s, all four columns on every one, the widest gap 102106 us, one config record carrying the bounds and start fill |
| AC-9, AC-14, AC-24 | `-p chorus-client-linux --test playout`, against the modelled device described in `tests/common/mod.rs` | the crossing is reported, the overflow counter is its own, occupancy never exceeds max plus one chunk, a stalled feed moves the underrun counter without widening a bound. **A model is not a sound card**; `tools/overflow-run.sh` is the run on a device that paces |
| AC-15 | `-p chorus-audio`, and over TCP | `bytes_discarded=3`, eleven chunks, sequence still contiguous |
| AC-16 | `make verify` (`tools/refusals.sh`) | exit 2, the format named, `chunks_sent=0`; same for an unsupported rate |
| AC-17 | `-p chorus-server --test serving`, on the chosen transport | a truncated chunk is a typed framing error and never becomes audio; a duplicate and a malformed frame are counted by reason with the session open and the underrun counter unmoved |
| AC-18 | `-p chorus-client-linux --test playout` | `discarded_late=1`, the chunk that was due is not displaced |
| AC-19 (first half) | `make verify` | exit 4, the device and the reason named, `usable=0 paces=0` |
| AC-20, AC-21 | `make verify-null-device` (`tools/stream-end-and-loss.sh` on `null`), through the real binaries | clean end: exit 0, in-band signal after final sequence 100, 96480 frames sent and 96480 written, `underruns=0`. Lost server: exit 3, `reason=connection-lost`, `played=1`, `underruns=0` |
| AC-22 (ceiling-zero half) | `make verify` | exit 3, both numbers named, nothing played; with the option, it starts and 4 of 4 status reports say so |
| AC-23 | `cargo test -p chorus-audio-path` | the committed tree passes both audio-path checks; both red demonstrations go red, in every module-declaration spelling. The same crate's `--test real_time_ordering` is a separate invariant, recorded under the host contract below |
| AC-25 | `make verify` (`tools/unrun-checks-are-visibly-unrun.sh`) | all environment-dependent entry points exit non-zero naming prerequisite and criterion, and the list is derived from the tools rather than restated. It was 8 when this was written and is 9 since RIG-3 added `tools/measure/capture-run.sh`, which is the derivation doing its job |
| AC-26 (denial half) | `make verify` | exit 3, the limit read and the amount wanted both named, nothing played; with the option, 4 of 4 status reports say so |
| AC-27 (start-up half) | `tools/start-fill-and-log-shape.sh` on `null`, and `make verify` | the three relations hold against the recorded config line: `min_us=60000 > 0`, `60000 < start_fill_us=120000 < 300000`, span 240000 us at 2000 ppm crosses in 120 s; a configuration that would not cross is refused at start |

**What the `null` device does not buy.** It accepts every frame instantly and
reports a delay of zero forever, so every sample in that one-minute log carries
`delay_us=0`. The run above is evidence about the start fill, the record's
shape and the bound relations, and about nothing whatever concerning the VALUE
of a device-reported delay. That is why `tools/delay-log-shape.sh` and
`tools/ten-minute-run.sh` refuse `null` by name.

### What did not run here, and what is filed instead

Every row below is a committed entry point that exits non-zero, naming the
prerequisite and the criterion, rather than reporting green. The refusals are
quoted verbatim from a run in this container.

#### AC-6, AC-8, AC-27 (the run-completion half): the ten-minute run

```
./tools/ten-minute-run.sh docs/measurements/ten-minute-run.log
```
with `CHORUS_CLIENT_DEVICE` pointed at real speakers. Missing here: an ALSA
device that reports a delay.

```
MISSING PREREQUISITE
  criterion:    ten continuous minutes with the reported delay inside its bounds and zero underruns
  prerequisite: an ALSA playback device that reports a delay; 'chorus-no-such-device' does not (...)
  how to get it: point CHORUS_CLIENT_DEVICE at a real card or a snd-aloop loopback; the ALSA 'null' device is not one, because it accepts every frame instantly and reports a delay of zero
  this check is NOT passed, NOT skipped-green and NOT satisfied.
```

Instructions: `docs/sound-2.md`, "The ten-minute run". The log it writes IS the
evidence and can be graded afterwards, on any machine, by someone who did not
run it:

```
./target/debug/chorus-delaylog-check <log> --min-graded-seconds 600 \
    --require-zero-underruns --require-no-rate-change
```

#### AC-5, AC-7 graded against a real delay, and AC-9, AC-14, AC-24 on a device

```
./tools/delay-log-shape.sh          # one minute, graded the way the long run is
./tools/overflow-run.sh             # the over-rate run, to the ceiling
```

Both need a device that paces; the refusal has the same shape as the one above,
with their own criterion lines. AC-5, AC-7 and AC-27's start-up half are
already answered above on `null`; what these two add is everything that rests
on the delay a device actually reports. Instructions: `docs/sound-2.md`, "The
client" and "The values this phase chose".

#### AC-10, AC-11, AC-12, AC-13, AC-26 (the locked half): the host contract

```
./tools/host-contract.sh
./tools/spin-test.sh
```

Missing here: a granted rtprio ceiling above zero.

```
MISSING PREREQUISITE
  criterion:    a real-time thread that runs without yielding past its CPU-time limit is terminated by that limit within one second, and a normal-priority process beside it keeps making progress
  prerequisite: a granted rtprio ceiling above zero; RLIMIT_RTPRIO reads 0 here
  how to get it: run in a container started with 'docker run --ulimit rtprio=<n>', as deploy/run-server.sh does
  this check is NOT passed, NOT skipped-green and NOT satisfied.
```

Instructions: `deploy/README.md`, "Checking the contract holds", and
`docs/sound-2.md`, "The spin test". What did run here: `-p chorus-hostctl`,
which covers the limits being read, `/proc/self/task` parsing, the
no-undeclared-real-time-thread check and both denial paths; and, in `make
test`, `cargo test -p chorus-audio-path --test real_time_ordering`, which
grades the CPU-time-bound ordering invariant against the committed
`real-time-acquisitions.conf`: every real-time acquisition site in the tree
applies `RLIMIT_RTTIME` earlier in the same function than it takes the
scheduling policy, an acquisition in a unit the file does not name is reported
as unaccounted for rather than skipped, and the three demonstrations go red for
an acquisition taken before its bound, for an unlisted acquisition, and never
for a name occurring only in prose.

That is a source scan and not a run of the scheduler, which is why it needs no
ceiling and appears here rather than in the rows above; what a granted ceiling
would add is `tools/spin-test.sh`, which is the entry point that makes the
bound fire.

##### What the suite now covers here with no privilege, and what still needs a ceiling

AC-13 is "no thread is real-time without being reported", and that answer is
only as good as the thread inventory it is read out of. The inventory is now
either complete or an error, and each way it can fail has its own test that
needs no privilege, no sound card and no real fault: the failure is injected at
the seam the enumeration reads the kernel through, with the errno the kernel
would have returned. **Covered here, by `cargo test -p chorus-hostctl`**
(`crates/hostctl/tests/thread_inventory.rs`, nine tests):

| behaviour | the test that holds it |
|---|---|
| one entry per live thread, carrying tid, name, policy and real-time priority, checked against a modelled listing, against a listing that LOSES a live thread on one pass (which `/proc/<pid>/task` was measured doing), and against this process's real `/proc/self/task` with four threads held alive on a barrier | `inventory_lists_every_live_thread` |
| a thread that exited between the listing and the read is omitted, the run still succeeds, and the omission is COUNTED so a caller can tell it from "nothing was dropped" | `vanished_thread_is_omitted_and_counted` |
| any other read failure fails the whole enumeration naming the thread and the reason, instead of shortening the list | `transient_read_failure_is_an_error_not_an_omission` |
| a refusal to list the threads, or to read one of them, fails the enumeration and hands no caller a partial list | `permission_denial_fails_the_enumeration` |
| a record that cannot be interpreted fails the enumeration naming that thread, rather than reading as a thread that is not there | `unreadable_scheduling_record_is_an_error` |
| an empty record, and a listing entry that names no thread, fall under the same rule and are never reported with default or zero values | `an_empty_record_is_never_reported_as_defaults` |
| an inventory with nothing in it is an error, because the thread doing the asking is itself a thread | `an_empty_inventory_is_an_error` |
| a name that cannot be read costs the thread its name and not its place, and is never the empty string | `an_unreadable_thread_name_does_not_drop_the_thread` |
| the undeclared-real-time-thread question is answered only from an inventory that completed, and an incomplete one is reported as the failure it is rather than as zero | `no_clean_answer_from_an_incomplete_inventory` |

Also covered here, by `make verify` (`tools/refusals.sh`): the host-contract
entry point, handed a report whose inventory did not complete, exits non-zero,
names this criterion and the reason, and reports nothing as passed or
skipped-green (`incomplete-inventory-*` checks), while still grading a complete
report on its merits (`complete-inventory-still-grades-clean`). Repeat-run
evidence for the suite is in
`docs/measurements/hostctl-thread-inventory-repeat.md`, which records 30
consecutive clean runs of `make test`, a 200-run before-and-after comparison
that took a real flake in `-p chorus-hostctl --lib` from 3.5% to zero observed,
the captured diagnosis of what was causing it, and a plain statement that a
clean streak bounds a flake rate rather than proving a flake gone.

**Still needs a granted rtprio ceiling above zero, and is NOT covered here.**
Everything about a thread that is actually real-time on this machine, because
no thread here can become one:

- that the priority obtained sits inside the granted ceiling, and is the
  running thread's actual scheduling priority (AC-10);
- that every real-time thread the server created is reported carrying a
  CPU-time bound, and that the bound FIRES rather than merely being configured
  (AC-11, AC-12, `tools/spin-test.sh`);
- that the count of real-time threads in the report matches the count the
  process actually has, against a process that has one (AC-13's positive half).
  The tests above establish that the inventory feeding that count is complete
  or is a refusal; they do not and cannot establish what the count is on a host
  that grants a ceiling.

`tools/host-contract.sh` remains the entry point for all of those and remains
in `tools/unrun-checks-are-visibly-unrun.sh`, so it still refuses visibly here.

#### AC-19 (the second half): a device removed mid-run

```
CHORUS_REMOVABLE_DEVICE=hw:Loopback,0 \
CHORUS_REMOVE_COMMAND='sudo modprobe -r snd_aloop' \
./tools/device-loss-run.sh
```

Missing here: a device that can be removed or made unusable mid-run.

```
MISSING PREREQUISITE
  criterion:    a device that becomes unusable during a run is reported with its reason, the client exits non-zero, and it never reports itself as playing while producing no audio
  prerequisite: a device that can be removed or made unusable mid-run
  how to get it: set CHORUS_REMOVABLE_DEVICE to an ALSA device you can unplug or unbind, and CHORUS_REMOVE_COMMAND to the command that removes it
  this check is NOT passed, NOT skipped-green and NOT satisfied.
```

Instructions: the script's own header. Nothing here guesses how to break a
device on someone else's machine. What did run here: the mid-run sink failure
against the modelled device, which exercises the client's half of it.

### Running the lot, on a machine that has the environment

```
make test                 # the suite: no device, no privilege
make verify               # the refusal paths, and that unrun checks are visibly unrun
make verify-null-device   # the device checks that do not grade the delay
make verify-device        # everything that needs a device (paces, for three of the four)
make verify-host          # the scheduling contract and the spin test
make ten-minute-run       # the evidence run
```

Each one either does the work or exits non-zero saying what it lacks. None of
them reports green for a check it did not run.
