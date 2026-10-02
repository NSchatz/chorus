# 0109: a third board profile is the emulator's, its link is `emulated` and it plays through nothing; Espressif's QEMU and its conda-forge libraries are pinned by sha256 and run as tools; the gate boots the image against a real server

- Status: accepted (goal 14, 2026-10-02)
- Decided by: the goal (program section 18, line B: "A QEMU esp32s3 good-bad-rollback run
  passes"; K93, I13), inside the goal-14 design envelope's section 7. This record is the first of
  the track's two changes: the board, the pinned emulator and a boot that is adopted. The
  good-image, bad-image and rollback run is the second and has its own record.
- Implemented in: `firmware/boards/qemu-s3-openeth.conf`, `firmware/sdkconfig.qemu-s3-openeth`;
  `firmware/include/chorus/wifi.h` (`CHORUS_TRANSPORT_EMULATED`),
  `firmware/include/chorus/link.h`, `firmware/src/link.c` (the third arm,
  `chorus_link_emulated_server`); `firmware/include/chorus/endpoint_config.h`,
  `firmware/src/endpoint_config.c`, `firmware/config/endpoint.conf` (`board_audio_output`);
  `firmware/main/esp_link.{c,h}` (the OpenCores binding), `firmware/main/app_main.c`;
  `tools/qemu/pins.conf`, `tools/qemu/libs.explicit.txt`, `tools/qemu/lib.sh`,
  `tools/qemu/flash_image.py`, `tools/qemu-env.sh`, `tools/qemu-boot-run.sh` (`make qemu-env`,
  `make qemu-boot`); `tools/conventions/check-pins.sh`; `tools/gate.sh` (steps
  `firmware-esp32s3-qemu` and `qemu-boot`); `tools/firmware-flash.sh` (refuses the emulator's
  image); held by `firmware/tests/test_link.c` (`make firmware-check`, target `link`) and
  `tools/conventions/check-flash-tools-refuse.sh`

## Context

Goal 14 owes a firmware update that survives a bad image, and nothing in this repository has an
ESP32-S3 to prove it on: flashing is the owner's act (K4). ESP-IDF's update path is not something
a host fake can stand in for entirely, because the part that decides a rollback is the
second-stage bootloader reading `otadata`. Espressif publishes a QEMU fork with an `esp32s3`
machine that runs the image as built, bootloader included. The goal's spike
(`.claude/goals/2026-09-chorus-g14.status.md` cites it) showed that it runs here and that the real
bootloader rolls back an unconfirmed slot.

For the endpoint image to boot there, three things about it had to give. The emulator has no I2C
and no I2S, so the amplifier's bring-up (which stops the endpoint before the network when the
part does not answer) cannot run. It has no general-purpose SPI and no radio, so neither the W5500
nor Wi-Fi is a link; it offers an OpenCores Ethernet controller instead. And its user network
carries no multicast, so the DNS-SD browse of ADR 0104 finds nothing.

The emulator itself is a new tool: GPL-2.0, a release archive from GitHub, and a binary that
needs three shared libraries this container's image does not ship and nobody here can install
with a system package manager.

## What was read

All read 2026-10-02.

- Pinned ESP-IDF v6.1 (`/cache/esp/esp-idf-v6.1`, Apache-2.0, commit
  `fff9895c82d744c7237be8847347bdd1b07c6643`): `tools/tools.json` (tool `qemu-xtensa`: version
  `esp_develop_9.2.2_20260417`, the archive's URL, sha256 and size, `license` `GPL-2.0-only`);
  `tools/idf_py_actions/qemu_ext.py:46-139` (the `esp32s3` target's arguments and its default
  eFuse bytes), `:244-333` (how `idf.py qemu` writes a fresh eFuse file and builds the command
  line); `docs/en/api-guides/tools/qemu.rst`; `components/esp_eth/Kconfig:147-181`
  (`ETH_USE_OPENETH`), `components/esp_eth/include/esp_eth_mac_openeth.h`,
  `components/esp_eth/include/esp_eth_phy.h`;
  `components/bootloader_support/include/esp_app_format.h:60-90` (the image header: the magic and
  the flash size code); `components/esp_netif/include/esp_netif.h` (`esp_netif_get_ip_info`);
  `docs/en/api-guides/build-system.rst:1097-1137` (`SDKCONFIG_DEFAULTS` takes a list).
- Espressif's QEMU documentation, not its source:
  https://github.com/espressif/esp-toolchain-docs/blob/main/qemu/README.md (the support table:
  for the ESP32-S3, I2C, I2S, general-purpose SPI, Wi-Fi and Bluetooth are not emulated; the UART,
  the flash, eFuses in a file, the RNG, the timers and OpenCores Ethernet are) and
  https://github.com/espressif/esp-toolchain-docs/blob/main/qemu/esp32s3/README.md (flash files of
  2, 4, 8 and 16 MB; the timer-group watchdog).
- QEMU's documentation of user networking,
  https://www.qemu.org/docs/master/system/devices/net.html ("Using the user mode network stack":
  the guest sits behind a "Firewall/DHCP server"; "The DHCP server assign addresses to the hosts
  starting from" the first guest address; "Note that ICMP traffic in general does not work with
  user mode networking").
- The emulator's own output, never its source: `--version`, `-machine help`, `-nic help`, and
  what it prints at run time. The release archive carries header files; none was opened.
- micromamba's release page, https://github.com/mamba-org/micromamba-releases/releases/tag/2.9.0-0
  (the `micromamba-linux-64.sha256` asset), and the conda package records
  (`conda-meta/*.json`: name, version, build, sha256, licence) of the environment the spike
  installed from https://conda.anaconda.org/conda-forge/linux-64.
- This repository: `tools/conventions/check-flash-tools-refuse.sh`,
  `tools/conventions/check-identity.sh`, `tools/conventions/check-pins.sh`,
  `docs/decisions/0057-board-profiles-and-the-wired-link.md`,
  `docs/decisions/0103-wifi-provisioning-over-softap.md` (the per-profile Kconfig fragment),
  `docs/decisions/0104-a-stored-identity-per-board-and-dns-sd-discovery-in-c.md`.

## Decision

**1. The emulator's board is a board profile like the other two.** `qemu-s3-openeth` is laid over
`endpoint.conf` by the same reader, embedded the same way, checked by the same configuration
gate, built by `tools/firmware-image.sh`, and held to the same safety scans and image guard. Its
model status is `confirmed`: the board is exactly the pinned emulator, so no owner's answer is
waited on. Its own Kconfig fragment (`firmware/sdkconfig.qemu-s3-openeth`, the mechanism of ADR
0103) sets two options and nothing else: the OpenCores controller on, and PSRAM off (the
emulator is started without any, and nothing the endpoint runs lives in external RAM). The
partition table, the flash size and everything the firmware update will add stay
`sdkconfig.defaults`', so the layout the emulator boots is a speaker's.

**2. A third transport, `emulated`.** `link_transport` takes `wired`, `wireless` or `emulated`.
`chorus_link_bring_up` runs the same three calls for it as for a wired link (init, start, wait
for an address), with the same refusal names, on the controller `app_main` bound: for this
transport that is `chorus_esp_link_emulated`, ESP-IDF's own OpenCores driver with its generic
PHY. No W5500 pin rule applies (there are no pins) and the radio is never touched. In an image
built without `CONFIG_ETH_USE_OPENETH`, which is every speaker's, the emulated init is a refusal
by name and none of the driver is linked.

**3. `board_audio_output = amplifier | none`.** A board key. `none` makes `app_main` bring up no
I2C bus, no I2S channel, no amplifier, no playout path, no sound chain and no fault watch; the
session then runs with no playout attached, which `firmware/src/session.c` already supports (the
host session binary runs so). The configuration check couples the key to the transport in both
directions: `none` on a wired or wireless board is the finding `no-audio-output-on-a-speaker`,
and an emulated board that declares an amplifier is `emulated-link-with-an-amplifier`. So no
speaker's profile can switch its amplifier's bring-up off, and the image refuses at boot too
(the board runs the same validation).

**4. The emulated board's server is its gateway.** The committed `server_address` is loopback
(the guest itself) and the browse finds nothing on a network without multicast. On QEMU's user
network the gateway the address lease names is the host, and a TCP connection to it reaches a
listener on the host's loopback. `chorus_link_emulated_server` (pure, tested) makes
`<gateway>:<the committed port>` from what `esp_netif` reports, and `app_main` hands that to the
session as its configured server; ADR 0104's order then takes it as "a static address that is
not loopback". No address is written in any tracked file, which the identity check would refuse
for an RFC 1918 address in any case.

**5. The emulated board logs the session's events on its console.** `event_log_path` is
`/dev/console` on this transport and stays unset on the others. The emulator is the one board
with nobody at its console, and the run that grades it reads the console.

**6. The emulator is a pinned tool, run and never read.** `tools/qemu/pins.conf` names the
release (`esp_develop_9.2.2_20260417`, the one the pinned ESP-IDF names for this machine), its
archive's URL and sha256, the sha256 of the program inside, and micromamba's version, URL and
sha256. `tools/qemu/libs.explicit.txt` lists the library environment as 56 conda-forge builds,
each with its sha256; micromamba refuses a package whose digest differs (tried with a wrong
digest: the install fails and nothing is created). `check-pins.sh` holds both files to that
form. `tools/qemu-env.sh install` fetches what is absent, checks each download, and extracts
under `/cache/opt` (or `CHORUS_QEMU_HOME`, `CHORUS_QEMU_LIBS`); with nothing absent it only
verifies: the program's sha256, every installed package against the list (none missing, none
extra, none differing), the version it reports, the `esp32s3` machine. Something present that
is not the pinned one is reported and never replaced. A run without the pinned emulator refuses
with `MISSING PREREQUISITE`; it is never a green skip (CLAUDE.md rule 8: an absent toolchain is
installed, not designed around). The libraries are put on the loader's path for the emulator's
command alone.

**7. The flash file is written without the flashing program.** `tools/qemu/flash_image.py`
copies the binaries `flash_args` lists to their offsets in a file of 0xFF, of the size the
build's own configuration names, and checks (does not rewrite) the bootloader header's flash
size. It writes one local file, talks to nothing and runs nothing. ESP-IDF's merge tool would do
the same and is the flashing program, which `check-flash-tools-refuse.sh` holds to the
owner-at-bench guard wherever it is named; that check is unchanged and still lists one tool.
`tools/firmware-flash.sh` now also refuses an image built for an emulated link, with or without
the owner at the bench, and the check runs that refusal.

**8. No eFuse is written, virtually either.** Every run makes a fresh eFuse file holding the
chip's defaults (the bytes ESP-IDF's own launcher writes, taken from the pinned tree at run
time) and fails unless the file's sha256 is the same after the emulator exits. Secure Boot,
Flash Encryption and anti-rollback stay refused by the scans this profile is built under.

**9. The gate.** `firmware-esp32s3-qemu` builds and scans the third profile;
`qemu-boot` runs `tools/qemu-boot-run.sh` on that build directory: a real `chorus-server` on
loopback, a first boot that is adopted under the id and key the board made and kept, the
emulator stopped, and a second boot on the same flash file that the server knows by the same id
and key.

## Evidence

Source: simulation. Nothing below is timing evidence, and nothing below says a board works.

EVIDENCE_PLACEHOLDER

## Licences

Nothing here is linked into, or shipped with, chorus. The emulator is a tool the gate runs; its
libraries are loaded by it.

- QEMU, Espressif's fork, `esp_develop_9.2.2_20260417`: GPL-2.0-only (`tools.json` of the
  pinned ESP-IDF). Run as an unmodified binary; its source is never read
  (`docs/clean-room.md`), which is how `mise.toml` already treats shellcheck, yamllint and
  cppcheck.
- micromamba 2.9.0-0: BSD-3-Clause.
- The library environment, as each conda package's own record states it:

| licence | packages |
|---|---|
| MIT | libdrm, libexpat, libffi, libpciaccess, libunwind, liburing, libxcb, libxml2, libxml2-16, pixman, pthread-stubs, wayland, xkeyboard-config, xorg-libx11, xorg-libxau, xorg-libxcursor, xorg-libxdmcp, xorg-libxext, xorg-libxfixes, xorg-libxi, xorg-libxrandr, xorg-libxrender, xorg-libxscrnsaver, xorg-libxtst |
| MIT AND MIT-open-group AND HPND AND HPND-sell-variant AND ISC | libxkbcommon |
| BSD-3-Clause | _openmp_mutex, libcap, libflac, libogg, libopus, libslirp, libvorbis, pcre2 |
| Zlib | libzlib, sdl2, sdl3 |
| 0BSD | liblzma |
| bzip2-1.0.6 | bzip2 |
| Apache-2.0 | libvulkan-loader |
| LicenseRef-libglvnd | libegl, libgl, libglvnd, libglx |
| LGPL-2.1-or-later | libglib, libsndfile, libsystemd0, libudev1, libusb, pulseaudio-client |
| LGPL-2.1-only | libiconv, mpg123 |
| LGPL-2.0-only | lame |
| GPL-3.0-only WITH GCC-exception-3.1 | libgcc, libgomp, libstdcxx |
| AFL-2.1 OR GPL-2.0-or-later | dbus |

## ASSUMED values

- The emulated PHY's address 1 and its 100 ms autonegotiation wait: the values the spike ran
  with; not from a datasheet (there is no part).
- `BOOT_SECONDS` 120 and `WAIT_SECONDS` 60 in `tools/qemu-boot-run.sh`: bounds on a run, not
  measurements.
- The board's flash size (8 MB, `board_flash_size_mb`) stays ASSUMED for the speakers; the
  emulator's flash file takes its size from the built image's own configuration.

## Deviations from the envelope

- The envelope's section 7 describes one script, `tools/ota-qemu-run.sh`. The coordinator split
  the track in two; this change adds `tools/qemu-boot-run.sh` and the shared `tools/qemu/lib.sh`
  the second will use.
- The research note expected the emulated board to use a static server address. An address in a
  tracked file is refused by the identity check, so the server is derived from the address
  lease at run time (Decision 4).
- The environment's lock is sha256 per package, not the md5 form the spike exported.

## Not chosen

- **Naming the merge tool and extending `check-flash-tools-refuse.sh` with an exemption.** It
  would have made the lexical rule "nothing but the guarded tool names a flashing program" into
  a rule with an exception to audit. The copy is about forty lines.
- **A base-image rebuild for the three libraries.** It is the operator's act and would make the
  gate depend on an image nobody here can change; the rootless environment is pinned as tightly
  and is the precedent `tools/lib.sh` already has for ALSA.
- **`idf.py qemu`.** It wraps the same command line, creates the flash file with the flashing
  program, and attaches a monitor; the run needs neither.
- **Emulating the octal PSRAM.** Possible behind another machine option, and nothing the
  endpoint runs would use it.
- **Skipping the amplifier by transport alone.** A key says what is skipped where a reader of
  the profile looks, and the validation ties the two together.

## Consequences and follow-ups

- The second change of this track adds the firmware-update run on the same board and the same
  library (`tools/ota-qemu-run.sh`).
- The emulator's watchdog is disabled, as ESP-IDF's own launcher disables it, so a hang is
  caught by `timeout`, not by a reset.
- The image reaches the server at the committed `server_address`'s port, so the run needs that
  loopback port free and refuses by name when it is not.
- CI installs the emulator with `tools/qemu-env.sh install` before `make gate`.

## Revisit when

- ESP-IDF moves its pinned emulator release (a toolchain upgrade names the new one).
- The emulator gains I2S or I2C for the ESP32-S3: the playout path could then run there too.
