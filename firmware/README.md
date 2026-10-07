# firmware - the ESP32-S3 endpoint

The second kind of endpoint chorus exists to have: a microcontroller speaker
that joins a group, disciplines its own playout to the same server timeline a
Linux endpoint does, drives a TAS5825M-class I2S amplifier, and comes back by
itself after the server or the link goes away.

Why it is shaped the way it is, and every constant it fixed:
`docs/decisions/0015-the-esp32-s3-endpoint.md`.

## The one thing to know first

**Every register-level constant of the amplifier cites its datasheet page.**
TI's TAS5825M datasheet (SLASEH7H rev H) is normative for the register map;
`config/endpoint.conf` carries the I2C address and every register and value
the bring-up uses, each line naming its page (`tools/conventions/check-amp-map.sh`),
and the reading fact by fact is `docs/research/tas5825m-register-map.md`
(`docs/decisions/0064-the-tas5825m-register-map-and-bring-up.md`). The address
strap stays ASSUMED until the owner's boards settle it. There is not one
register literal in `src/amp.c`, and `check/endpoint_scan.c` fails the suite if
one appears: the driver reads every constant from the configuration.

## Boards and the link

P1 (approved at Checkpoint K): an ESP32-S3 in every speaker. The wired classes
reach the network through a W5500 on SPI with its interrupt line wired, and
that is the default link; the compact speakers may run the S3's own Wi-Fi (the
Wi-Fi tier, K91). A board is a profile under `boards/`, laid over
`config/endpoint.conf` (which carries the default profile's values) and
embedded in the image:

```
boards/brick-s3-wired.conf    the default: the wired classes, W5500 on SPI2
boards/compact-s3-wifi.conf   the compact speakers' Wi-Fi tier
boards/qemu-s3-openeth.conf   not a speaker: the emulator's board (goal 14)
boards/devkitc-s3-louderhat-wired.conf
                              the compact on three bought modules: DevKitC-1-N8R8,
                              Louder Raspberry Hat Plus (TAS5825M), WIZ850io (W5500),
                              with its buttons, status light and microphone
boards/devkitc-s3-louderhat-twoway.conf
                              the two-way on the same three modules, with its
                              pairing button and status light
```

The first two name P1's bought reference board, the Esparagus Audio Brick (ESP32-S3), and
mark it **ASSUMED** until the owner's own boards are identified (the owner step
"Your ESP32-S3 boards: module markings and a read-only chip report", which
`board_needs_item` names).
`CHORUS_BOARD_PROFILE=compact-s3-wifi make firmware-image` builds the Wi-Fi
image; `make gate` builds every profile. The third is Espressif's QEMU `esp32s3`
machine: its link is `emulated` (the emulator's OpenCores Ethernet), it plays
through nothing (`board_audio_output = none`; the emulator has no I2C and no
I2S), and `make qemu-boot` boots its image against a real server
(`docs/decisions/0109-an-emulator-board-profile-and-the-pinned-emulator.md`);
`tools/firmware-flash.sh` refuses to write that image to a device. The fourth is
the compact speaker on three bought modules (the Brick is no longer sold), on the
reference board's pins plus the compact's buttons, light and microphone
(`docs/decisions/0230-the-compact-on-bought-modules.md`). The W5500 driver is the Component Registry's
`espressif/w5500`, pinned in `main/idf_component.yml` and `dependencies.lock`.
Why: `docs/decisions/0057-board-profiles-and-the-wired-link.md`.

## The playout path

`src/playout.c` is the endpoint's jitter buffer and the loop that disciplines
it (chorus goal 8, audit A-9). Its device delay is measured at the DMA: the I2S
TX `on_sent` interrupt reports each DMA buffer it finished, and the hook counts
those frames and stamps them there on the monotonic clock. Written minus
consumed, less the part of the current buffer played since the stamp, is how
far the next frame written is from the pins; the error and the servo are the
Linux client's, and the correction inserts or drops frames. Its constants come
from `config/sync.conf`, embedded in the image (audit A-12).
`tests/test_playout.c` grades it on a host against a fake DMA on a fake clock
(`make -f firmware/Makefile playout`); that is a model, not timing evidence.
Nothing has been heard: the binding in `main/esp_playout.c` and
`main/esp_hal.c` is compiled, not run.

## Running it

```
make firmware-check          # everything, on a host. No ESP32-S3, no ESP-IDF
                             # toolchain (only the pinned tree's TF-PSA-Crypto
                             # sources, ADR 0043), no amplifier, no sound card,
                             # no privilege.
make firmware-image          # the image. Refuses by name without the ESP-IDF
                             # toolchain at the version endpoint.conf declares,
                             # and emits no partial image.
make verify-endpoint-rig     # AC-1 and AC-3. NOT PASSED, and this refuses by
                             # name until somebody has the hardware.
```

`make firmware-check` takes about three minutes, and most of that is one
number: `outage_minutes_seconds` in `config/endpoint.conf` is 130, and the
outage-of-minutes run really does leave a real server dead for that long. A
modelled minute cannot exhaust a retry budget, which is the whole property that
run grades.

## The tree

```
config/endpoint.conf     every value the endpoint needs, committed once
boards/                  board profiles laid over it (board keys only)
endpoint-units.conf      the endpoint's source, enumerated, and what the scans
                         are true of
include/chorus/          the headers
src/                     the endpoint. Shared by the host build and the image
                         build, byte for byte
main/                    the ESP-IDF binding: app_main and the driver-backed
                         implementations of the three injectable interfaces
check/                   host tooling: the configuration gate and the safety
                         scans. Deliberately OUTSIDE the tree it scans
tests/                   the host suites, the simulated amplifier, and the
                         outage harness
crypto/                  the host build's TF-PSA-Crypto configuration. The
                         library itself is compiled from the pinned ESP-IDF
                         v6.1 tree (CHORUS_IDF_V61_DIR, default
                         /cache/esp/esp-idf-v6.1), never copied here
```

Protocol v2's key exchange needs a crypto library, and the endpoint reaches it
only through the PSA Crypto API (`psa/crypto.h`). The image gets it from
ESP-IDF's mbedtls component; the host build compiles TF-PSA-Crypto 1.1.0 out of
the same pinned ESP-IDF v6.1 checkout (commit
fff9895c82d744c7237be8847347bdd1b07c6643) with `crypto/chorus_psa_config.h`,
and refuses by name when that tree is absent. On the host the endpoint's key
and its server pins are files (`--key`, `--server-pins`); the image keeps both
in NVS through `chorus/store.h` (`main/esp_store.c`).

## What is graded here, and what is not

Graded on a host, with no hardware at all:

| what | how |
|---|---|
| the protocol core | every vector pair FOUND under `fixtures/protocol/`, in both directions, byte for byte and field for field, plus the decoder-behaviour order `docs/protocol.md` fixes. The directory is enumerated, so a committed pair with no C mirror goes red naming the type |
| protocol v2 | every vector FOUND under `fixtures/protocol/v2/` (`tests/test_protocol_v2.c` prints `v2 golden vectors: N of N passed`), the value rules refused by the encoder and rejected by the decoder as the same field, every single-byte corruption of every vector, and every prefix of a stream of them |
| the key exchange | `Noise_XX_25519_ChaChaPoly_SHA256` (`src/noise.c`, PSA Crypto calls only) reproduces the published cacophony vector as initiator and as responder, and the four chorus session vectors (`handshake_init`, `handshake_response`, `handshake_finish`, `secure_record`) from their public test keys (`tests/test_noise.c`) |
| the sync core | every scenario under `fixtures/sync` drives the error below its bound and holds it, AND every exchange reproduces `fixtures/sync/crosscheck/` exactly |
| the bring-up order, and every unwind | a simulated part, a simulated output stage and a simulated I2S controller writing one event log the driver cannot reach. Stopping a clock counts as a clock change, so the teardown paths are held to the same rule as the first one: high impedance before it, always |
| the gain ceiling, and every failure of the bus | the same simulated part |
| the clock rules and the pin map | at build time. `make firmware-check` compiles nothing until `chorus-endpoint-config-check` has passed over `config/endpoint.conf` |
| the safety scans | over the whole endpoint tree, with eight demonstrations that go red on a smuggled instance |
| the rejoin | a real loopback socket, the `chorus-server` at HEAD killed with SIGKILL and replaced, three outage shapes, every connection a protocol v2 session; and a fourth shape, a server that comes back with another key, which the endpoint refuses (`session_refused` `key_changed`) and stops |

NOT graded here, and not claimed:

- **AC-1**, that an ESP32-S3 endpoint holds inter-device error within the bound
  SYNC-4 met beside a Linux endpoint. Needs an ESP32-S3, an amplifier, a
  loudspeaker, a Linux endpoint and the RIG-3 capture rig.
- **AC-3**, that the 24-bit configuration produces the sample rate it asks for.
  Graded by MEASURING the produced rate, because a configuration read back
  agrees with itself whatever the hardware does. The half a host can check -
  that the MCLK multiple is divisible by three - is enforced at build time.
- **`main/`**, the ESP-IDF binding, which is compiled only by ESP-IDF.

`tools/endpoint-rig-run.sh` is the entry point for the first two. It exits
non-zero naming what is missing, and `docs/verification-record.md` quotes that
refusal and says plainly that neither criterion is passed.

## Adding to it

- A new unit under `src/`, `include/` or `main/` is enrolled by being placed
  there: the scan walks those directories and fails on anything the list does
  not account for.
- A new sync scenario is a new file under `fixtures/sync`, plus
  `make sync-vectors` to render its cross-check vector.
- A new configuration value goes in `config/endpoint.conf` and is read by
  `src/endpoint_config.c`. If it can be wrong in a way that damages something,
  add a rule for it in `src/i2s.c` and a red demonstration in
  `tests/test_i2s.c`.
