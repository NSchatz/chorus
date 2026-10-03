# 0111: the firmware update is run under the emulator in `make gate`: three images of the emulator's board, the server's explicit install of a good one (confirmed) and of a bad one built never to confirm (rolled back by ESP-IDF's bootloader), with a power cycle between them

- Status: accepted (goal 14, 2026-10-03)
- Decided by: the goal (program section 18, line B: "A QEMU esp32s3 good-bad-rollback run
  passes"; K93, I13), inside the goal-14 design envelope's section 7. The second of track
  ota-qemu's two changes; the first is ADR 0109.
- Implemented in: `tools/ota-qemu-run.sh` (`make ota-qemu`), the gate step `ota-qemu` in
  `tools/gate.sh`; `tools/firmware-image.sh` (`CHORUS_IMAGE_VERSION`, `CHORUS_OTA_NEVER_CONFIRM`,
  the recorded board profile), `firmware/main/CMakeLists.txt` (the never-confirm definition,
  refused for every profile but the emulator's; no profile copy during early expansion);
  `tools/unrun-checks-are-visibly-unrun.sh`; the report
  `docs/measurements/ota-qemu-rollback.md`

## Context

ADR 0108 put the update's decisions in a pure unit graded over a fake flash that models the
bootloader's rule, and ADR 0110 made the server stage, verify and install images only on an
explicit command. Neither runs ESP-IDF's `app_update` or its second-stage bootloader, which is
what actually decides a rollback on a board. ADR 0109 gave the gate an emulator board that boots
the real image. This change runs the update end to end on it.

## What was read

All read 2026-10-02 and 2026-10-03.

- Pinned ESP-IDF v6.1 (`/cache/esp/esp-idf-v6.1`, Apache-2.0, commit
  `fff9895c82d744c7237be8847347bdd1b07c6643`): `tools/cmake/project.cmake:813-830` (the
  project version: `PROJECT_VER` when defined, else the version file or the git description),
  `docs/en/api-guides/build-system.rst:1684` (early expansion runs each component's
  `CMakeLists.txt` in script mode with `CMAKE_BUILD_EARLY_EXPANSION` defined);
  `components/esp_app_format/include/esp_app_desc.h` (the application description: the version
  at offset 16, 32 bytes); `components/bootloader_support/include/esp_app_format.h` (the image
  and segment headers).
- This repository: `docs/decisions/0108-the-ota-state-machine-and-the-firmware-wire.md`,
  `docs/decisions/0110-explicit-firmware-installs.md`, `docs/firmware-updates.md`,
  `firmware/main/esp_ota.c`, `tools/firmware-stage.sh`, `tools/firmware-flash.sh`,
  `docs/decisions/0109-an-emulator-board-profile-and-the-pinned-emulator.md`.
- The pinned emulator's own output only; its source is never read (ADR 0109).

## Decision

**1. Three images of the emulator's profile.** A is the image the gate's
`firmware-esp32s3-qemu` step built (version: ESP-IDF's default, the git description); GOOD is
built with `CHORUS_IMAGE_VERSION=ota-qemu-good`; BAD with `CHORUS_IMAGE_VERSION=ota-qemu-bad` and
`CHORUS_OTA_NEVER_CONFIRM=1`, which compiles ADR 0108's `never_confirm` into the update unit
(`esp_ota.c` already reads the definition). `tools/firmware-image.sh` refuses
`CHORUS_OTA_NEVER_CONFIRM=1` for any profile whose link is not `emulated`, and the component's
CMake refuses it again, so no speaker's image is ever built to fail its trial. GOOD and BAD keep
build directories of their own; a directory remembers which version and which variant it was
configured with and is configured afresh when that changes (CMake keeps `-D` values in its
cache).

**2. Driven by the server's explicit install, graded on what the board reads and what the
server says.** GOOD and BAD are staged with `tools/firmware-stage.sh` and verified by the server
at start-up. The run checks that nothing is offered while nobody asks (`update_available` true,
no `firmware offer` in the server's log), then sends `firmware_install` GOOD, then BAD, through
`POST /api/command`. It grades on the board's own boot report (`chorus-ota: running slot=<n>
state=<otadata state> version=<v>`, which `esp_ota.c` prints from `esp_ota_get_state_partition`
at every boot: the state the bootloader left, read by the application, as the spike advised
rather than a bootloader wording) and on the server's `/api/state` (`speakers[].firmware`).
GOOD: slot 1 `pending-verify`, then the server's `confirmed`. BAD: slot 0 `pending-verify`, the
unit's own "did not confirm in time", then slot 1 `valid` running GOOD again, and the server's
`rolled_back` with reason `not_confirmed`, version GOOD, image BAD.

**3. A power cycle between the two installs.** In one emulator process an image written over
a slot that process had already executed crashed as the bootloader jumped to it (Evidence).
The emulator is therefore stopped and started on the same flash file once GOOD is confirmed,
and the run checks GOOD boots `valid`. A board loses its power too; what the run does not
exercise is a bad image written over a slot the same power-on already ran.

**4. The build directory records the profile it was built for.** The emulator run found that
`<build>/board_profile.conf`, which `tools/firmware-flash.sh` and `tools/firmware-stage.sh` read
as the profile the build recorded, was written by ESP-IDF's early expansion with the DEFAULT
profile whatever was built: the first GOOD and BAD were staged as `brick-s3-wired` and refused
`wrong-board`. The component no longer copies the profile during early expansion, and
`tools/firmware-image.sh` copies the embedded one there after the build. So the flash tool's
refusal of a profile mismatch, and of an emulator image (ADR 0109), read the real profile.

## Evidence

Source: simulation. Not timing evidence. The run's tail, the serial lines and the server's
lines are in `docs/measurements/ota-qemu-rollback.md`.

The crash that Decision 3 avoids, from the run before it (one emulator process: A on slot 0,
GOOD to slot 1 and confirmed, then BAD written to slot 0):

```
I (39023) chorus-ota: rebooting into the new image
I (43850) boot: Loaded app from partition at offset 0x20000
Guru Meditation Error: Core  0 panic'ed (IllegalInstruction). Exception was unhandled.
PC      : 0x3c0f3c3d  ...
I (44360) boot: Loaded app from partition at offset 0x320000
I (18585) chorus-ota: running slot=1 state=valid version=ota-qemu-good
```

The bootloader still rolled back (the image rebooted on trial without being marked valid), but
for a crash, not for the trial; the run counts that as a failure. With the power cycle, BAD
booted on slot 0, ran its trial and gave itself up, and nothing panicked.

## ASSUMED values

- The trial is `ota_confirm_seconds` (60, endpoint.conf; ADR 0108). The run waits that plus 60 s.
- `EMULATOR_SECONDS` 900, `WAIT_SECONDS` 60, `TRANSFER_SECONDS` 300: bounds on a run.
- The image names `ota-qemu-good` and `ota-qemu-bad`: any two versions that differ from A's.

## Deviations from the envelope

- Section 7 names versions 1, 2 and 3. The versions here are A's git description,
  `ota-qemu-good` and `ota-qemu-bad`: the server compares versions as names (ADR 0110), and a
  name says which image a log line is about.
- The power cycle between the installs (Decision 3) is not in the envelope.
- Decision 4 changes `firmware/main/CMakeLists.txt` and `tools/firmware-image.sh` beyond the
  build hooks; it fixes a defect the run found in files other tracks read.

## Consequences and follow-ups

- The owner's bench session (S9) is still the hardware demonstration: a real board, a real
  rollback.
- The emulator's stale-code behaviour is noted here and in the run; a later emulator release
  can be tried without the power cycle.
