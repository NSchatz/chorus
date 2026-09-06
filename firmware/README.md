# firmware - the ESP32-S3 endpoint

The second kind of endpoint chorus exists to have: a microcontroller speaker
that joins a group, disciplines its own playout to the same server timeline a
Linux endpoint does, drives a TAS5825M-class I2S amplifier, and comes back by
itself after the server or the link goes away.

Why it is shaped the way it is, and every constant it fixed:
`docs/decisions/0015-the-esp32-s3-endpoint.md`.

## The one thing to know first

**Every register-level constant of the amplifier is DECLARED UNKNOWN.** TI's
datasheet is normative for the TAS5825M register map and the research pass
could not extract text from the PDF, so this phase asserts the bring-up
BEHAVIOUR and names no address. `config/endpoint.conf` carries the literal word
`unknown` for the I2C address and every register, and the bring-up sequencer
refuses by name, leaves the output stage in high impedance and starts no I2S
clock rather than guessing. There is not one register literal in
`src/amp.c`, and `check/endpoint_scan.c` fails the suite if one appears.

Read them off the datasheet at bring-up and write them into
`config/endpoint.conf`. Nothing else needs to change.

## Running it

```
make firmware-check          # everything, on a host. No ESP32-S3, no ESP-IDF,
                             # no amplifier, no sound card, no privilege.
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
```

## What is graded here, and what is not

Graded on a host, with no hardware at all:

| what | how |
|---|---|
| the protocol core | `fixtures/protocol/` in both directions, byte for byte and field for field, plus the decoder-behaviour order `docs/protocol.md` fixes |
| the sync core | every scenario under `fixtures/sync` drives the error below its bound and holds it, AND every exchange reproduces `fixtures/sync/crosscheck/` exactly |
| the bring-up order | a simulated part, a simulated output stage and a simulated I2S controller writing one event log the driver cannot reach |
| the gain ceiling, and every failure of the bus | the same simulated part |
| the clock rules and the pin map | at build time. `make firmware-check` compiles nothing until `chorus-endpoint-config-check` has passed over `config/endpoint.conf` |
| the safety scans | over the whole endpoint tree, with eight demonstrations that go red on a smuggled instance |
| the rejoin | a real loopback socket, a real `chorus-server` process killed with SIGKILL and replaced, three outage shapes |

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
