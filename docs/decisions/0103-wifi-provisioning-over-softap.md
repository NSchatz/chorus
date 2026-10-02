# 0103: a compact Wi-Fi speaker learns its network from a phone over its own access point and its own join page, keeps it in one small store, and ESP-IDF's provisioning manager rides along for Espressif's clients

- Status: accepted (goal 14, 2026-10-02)
- Decided by: the goal (program section 18, line E: "Wi-Fi provisioning works on the host build and
  its phone step is a packet in the owner's queue"; K91, K92, K16, K27), inside the goal-14 design
  envelope's sections 1 and 6
- Implemented in: `firmware/include/chorus/store.h`, `firmware/src/store.c`,
  `firmware/main/esp_store.{c,h}` (the store seam and its NVS binding);
  `firmware/include/chorus/provision.h`, `firmware/src/provision.c` (the decisions);
  `firmware/main/esp_provision.{c,h}` (the binding, Wi-Fi profile only);
  `firmware/main/idf_component.yml` and `firmware/dependencies.lock`
  (`espressif/network_provisioning ==1.2.5`); `firmware/sdkconfig.compact-s3-wifi` and the
  per-profile fragment mechanism in `tools/firmware-image.sh`; two calls in
  `firmware/main/app_main.c`; held by `firmware/tests/test_provision.c` with
  `firmware/tests/fake_store.{c,h}` and the access point added to `firmware/tests/fake_radio.{c,h}`
  (`make firmware-check`, target `provisioning`); the owner's step is `docs/bench-packet.md` S9

## Context

The compact speakers run on Wi-Fi (K91). The repository declares `link_wifi_ssid` and
`link_wifi_secret` `unknown` and the wireless bring-up refuses to join while they read so
(ADR 0024, audit A-10), so a Wi-Fi image could be built and could never join anything. Goal 14
owes the path by which a speaker learns its network at run time, without a network's name or
passphrase ever entering a tracked file or a log (K27), driven in the end by a web app and never
a native one (K16), with adoption on the audio network as the success signal (K92). Three firmware
tracks of the same goal also need somewhere to keep things across boots (the endpoint id and
Noise key, the server pins, the OTA note); this record carries that seam because this track
landed it.

## What was read

All read 2026-10-02. The goal's research file (provisioning) was re-verified claim by claim
against the sources below before anything was built on it.

- Pinned ESP-IDF v6.1 (`/cache/esp/esp-idf-v6.1`, Apache-2.0, commit
  `fff9895c82d744c7237be8847347bdd1b07c6643`):
  `docs/en/migration-guides/release-6.x/6.0/provisioning.rst:9` (`wifi_provisioning` removed from
  the tree and renamed `network_provisioning`), `:40-52` (protocomm security 0 and 1 now default
  to n); `docs/en/api-reference/provisioning/provisioning.rst:70-85` (SoftAP against BLE), `:100`
  ("Unique per-device passphrase can also act as a proof-of-possession");
  `components/protocomm/Kconfig:3-27`, `components/protocomm/CMakeLists.txt:1-5` (returns early on
  the linux target), `:30-58` (what each security version compiles, its requirements);
  `components/protocomm/include/transports/protocomm_httpd.h:19-23`,
  `components/protocomm/src/transports/protocomm_httpd.c:258-272` (an HTTP server handed in from
  outside); `components/esp_wifi/Kconfig:455-457`, `components/lwip/Kconfig:409-411`,
  `components/mbedtls/Kconfig:892-894, 943-945`, `components/esp_http_server/Kconfig:4-18`,
  `components/esp_http_server/include/esp_http_server.h:60-70`;
  `components/esp_wifi/include/esp_wifi_types_generic.h:128-173, 564-565` (disconnect reasons;
  `ssid[32]`, `password[64]`); `components/esp_wifi/include/esp_wifi_default.h:83`;
  `components/esp_netif/include/esp_netif.h:403, 953`;
  `docs/en/api-reference/storage/nvs_flash.rst:33, 46, 105`,
  `components/nvs_flash/include/nvs.h:60, 167, 375, 544, 585, 638`,
  `components/nvs_flash/include/nvs_flash.h:77`;
  `docs/en/api-guides/build-system.rst:1097-1137` (`SDKCONFIG_DEFAULTS` takes a list).
- `espressif/network_provisioning` 1.2.5 as the component registry serves it
  (https://components.espressif.com/api/components/espressif/network_provisioning ; archive
  `espressif__network_provisioning-v1.2.5.zip`, component hash `8b2be80f...`, published
  2026-09-23; upstream commit `9db0a7a7787cf12184aa8948d4e8e1985ed83cda` per its manifest):
  `idf_component.yml`, `CMakeLists.txt`, `Kconfig`, `include/network_provisioning/manager.h`,
  `include/network_provisioning/scheme_softap.h`, `src/manager.c` (lines 620-700, 1225-1325,
  1740-1845, 2085-2440), `src/scheme_softap.c`, `CHANGELOG.md` ("1.2.5 (9-September-2026): Fix
  buffer overreads when Wi-Fi SSIDs or passwords are not null-terminated") and `LICENSE`.
- That protocomm's HTTP transport sends no CORS header: a case-insensitive search for `cors` and
  `Access-Control` over `components/esp_http_server` and `components/protocomm` of the pinned
  tree finds nothing.
- Web Bluetooth on phones:
  https://raw.githubusercontent.com/Fyrd/caniuse/main/features-json/web-bluetooth.json (iOS
  Safari "n" for every version listed, through 27.2; status "unoff").
- The licence: the registry's API labels 1.2.5 "Custom" (1.2.4 was labelled "Apache-2.0"). The
  licence text it serves for 1.2.5
  (https://components-file.espressif.com/components/espressif/network_provisioning/1.2.5/license.txt)
  is 11365 bytes, sha256
  `f0f612d1627194ddd07c8b5b238a514ae9794632e5470c00f16237def7dc2246`, begins "Apache License,
  Version 2.0, January 2004", and is byte-identical to the `LICENSE` inside the archive. The
  component is Apache-2.0; only the label changed. Its dependency resolved to `espressif/cjson` 1.7.19~2
  (hash `e7883232...`, in `dependencies.lock`), labelled MIT by the registry
  (https://components.espressif.com/api/components/espressif/cjson).
- chorus: `firmware/include/chorus/wifi.h`, `firmware/src/wifi.c`, `firmware/tests/fake_radio.*`,
  `firmware/tests/test_wifi.c`, `firmware/main/{app_main.c,esp_hal.c,console_esp.c,CMakeLists.txt}`,
  `firmware/Makefile`, `firmware/endpoint-units.conf`, `firmware/check/efuse-kconfig.list`,
  `tools/firmware-image.sh`, `tools/gate.sh`, `tools/conventions/check-identity.sh`,
  `docs/bench-packet.md`, ADR 0015, 0024, 0057, 0060.

No GPL source was opened.

## Decision

**1. The store seam.** `chorus/store.h` is the goal-14 envelope's header: get, set and erase of
whole values under keys of 1 to 15 bytes of `[a-z0-9_]` (NVS's key limit), values of at most 1024
bytes. `store.c` decides the key rule and the bounds in front of the medium, so a key the board
would refuse is refused on the host too, and does not believe a medium that claims more bytes
than the caller's buffer. On the board the medium is NVS, namespace `chorus`, one blob per key,
committed after every change (`esp_store.c`). `chorus_esp_store_init` never erases the partition:
what it holds is the speaker's identity and its network, and an image that erased them on an NVS
error would silently re-adopt and re-provision. NVS encryption stays off (it is on the refused
Kconfig list: it needs Flash Encryption or an eFuse key), so everything stored, the house's Wi-Fi
passphrase included, is readable by anyone who can read the flash chip. That is the price of
guardrail 2 on development hardware and is stated here rather than discovered later.

**2. SoftAP, not BLE.** The client that must work is a web page on any phone (K16). Web
Bluetooth does not exist on iOS, BLE would put a Bluetooth stack in an image K53 keeps Bluetooth
out of, and software coexistence makes Wi-Fi sleep outside its time slice, which the wireless
bring-up treats as a reason to withhold the bound (ADR 0024). SoftAP costs the phone a manual
network change and a status that may not arrive; adoption (K92) is the better success signal
anyway.

**3. The speaker serves its own join page.** protocomm's HTTP transport sends no CORS headers
and a secure page cannot portably fetch a plain-HTTP device, so the web app cannot drive
protocomm; a page served by the speaker on its own access point works in every browser. `GET /`
is a two-field form and `POST /join` takes it (`application/x-www-form-urlencoded`: `ssid`,
`secret`). The parser is pure C, host-tested on hostile input, and refuses by name: a body over
384 bytes, a malformed pair, a bad escape, a zero byte, an unknown or duplicate field, a name
outside 1 to 32 bytes, a passphrase outside 8 to 63 printable ASCII characters or 64 hexadecimal
digits. An open network is refused (`secret-too-short`). The pages are fixed text with no script;
the only variable text is a refusal's name, and nothing a phone sent is ever echoed.

**4. The setup secret.** At first boot the speaker draws 12 characters from a 31-symbol alphabet
without look-alikes (about 59 bits, rejection-sampled from the hardware generator through the
PSA call the session's keys use) plus 6 more for the access point's name
(`chorus-setup-<6>`), and keeps both under `prov_secret`. The secret is the access point's WPA2
passphrase AND protocomm's proof of possession; ESP-IDF's guide accepts exactly that. It is
printed on the serial console, once per boot that raises the access point, by the binding. A
secret that cannot be stored is refused rather than used once: the owner would be handed a
passphrase that changes every boot.

**5. Where a credential may go.** To the radio, and to the store (`wifi_ssid`, `wifi_secret`),
and only after the join worked. Never to a log line, a page, or the store's event log; the
network's NAME is kept out of the unit's lines too, because a console log pasted back from the
bench would carry it (K27): a line says `ssid_bytes=<n> secret_bytes=<n>`. A refused form or a
failed join wipes the passphrase from the unit. `link_wifi_ssid/secret` stay `unknown` in
`endpoint.conf` and now mean "provisioned at run time": the binding writes the stored network
into the link configuration in RAM for the bring-up that follows.

**6. The states.** `unprovisioned`, `ap-up`, `credentials-received`, `joining`, `joined`,
`join-failed`, `provisioned-at-boot`, `reset`, plus `wired` (a wired profile never provisions:
no store read, no radio, no access point) and `refused` (the store or the access point would not
work; named). Every join is `chorus_wifi_bring_up`, so the power save mode is set and read back
before any join here as everywhere. With a stored network a boot tries it three times and only
then raises the access point, keeping the stored network (a router that is off is not a wrong
passphrase) and the reason for the page; the access point then tries the stored network again
every 120 s by itself, so a speaker that booted faster than the router after a power cut needs
nobody. A network from the form is joined once; a failure returns to `ap-up` with the platform's
word (`auth-error`, `network-not-found`) and never overwrites what was stored. The reset (the
console's `wifi-reset`; `chorus_esp_provision_request_reset` for the pairing button's long hold
when the controls get a GPIO binding) erases the two network keys and nothing else, so the access
point returns under the same name and secret.

**7. ESP-IDF's manager, for Espressif's clients.** `espressif/network_provisioning` is pinned
`==1.2.5` (the release with the fix for overreads on unterminated names and passwords; 1.3.x was
one and two days old and swaps cJSON for a dependency major published the same day). It is
started on the speaker's own HTTP server (scheme SoftAP, security 1, the setup secret as proof of
possession), so Espressif's phone app and `esp_prov` work as a cross-check. A network that arrives
that way is taken from the manager's event, checked and stored by the same unit; the manager owns
that join and its verdict is the join's. The browser path does not go through the manager's
`configure_wifi_sta`: the binding joins with the Wi-Fi driver's storage set to RAM, so the network
lands in chorus's store and nowhere else. The manager writes a network it receives to the Wi-Fi
driver's own NVS namespace as well; the binding empties that copy at the next boot.

**8. Per-profile Kconfig.** `tools/firmware-image.sh` passes
`-D SDKCONFIG_DEFAULTS="firmware/sdkconfig.defaults;firmware/sdkconfig.<profile>"` when the
second file exists (any profile; the goal's QEMU profile reuses it), on the command line because
the scans refuse it in CMake, named `sdkconfig.*` under `firmware/` so scan rule 9 reads it. The
generated `sdkconfig` in a persistent build directory is regenerated when the committed files it
was made from change, because ESP-IDF reads defaults only for options the existing `sdkconfig`
does not hold. `firmware/sdkconfig.compact-s3-wifi` turns protocomm security 1 on (default n
since v6.0), security 2 and 0 off, and states SoftAP support, the DHCP server and AES-CTR.
`esp_provision.c` is compiled only when the board profile's `link_transport` is `wireless`.

## Evidence

- `make firmware-check`, target `provisioning`: `test_provision: 138 checks, 0 failed`: the store
  seam's key rule and bounds; the form parser on good bodies and on 25 named hostile ones, a
  megabyte of body, and every prefix and eight single-byte changes at every position of a good
  body; every state and transition
  read off the unit's trace and off the fake radio and fake store; the setup secret; and the full
  run (first boot unprovisioned, access point up, form posted, the fake radio's network joined,
  credentials stored, a "reboot" that joins with no access point, a failed join back to the
  access point with the reason, a reset that erases). The last section greps the unit's lines and
  the store's event log for the network name, the passphrase and the setup secret.
- Both images build and pass the image guard with the component in the build
  (`firmware-esp32s3-wired`, `firmware-esp32s3-wifi`). Image sizes, from each build's own
  `check_sizes.py` line (app partition 0x177000 = 1,536,000 bytes):

  | profile | before (origin/main `0ea250d`) | after | delta | free after |
  |---|---|---|---|---|
  | compact-s3-wifi | 1,367,968 (0x14dfa0) | 1,498,096 (0x16dbf0) | +130,128 | 37,904 (2%) |
  | brick-s3-wired | 1,368,672 (0x14e260) | 1,369,216 (0x14e480) | +544 | 166,784 (11%) |

  Where the Wi-Fi image's growth is, from `python -m esp_idf_size --archives --diff` over the two
  map files (bytes): the manager 28,714; string constants 27,636 (the linker merges them into one
  section the tool books under `libesp_stdio.a`); chorus's own units and binding 15,122 (5,596 of
  it static RAM); the HTTP parser 15,029; protocomm 11,206; the HTTP server 10,943; protobuf-c
  10,072; the Wi-Fi library's access-point paths 5,268; lwIP (the DHCP server) 4,012; cJSON 3,037.
  The wired image grew by the store alone. THE WI-FI IMAGE HAS 37,904 BYTES LEFT in the 1.5 MB
  single-app partition: enough to merge, not enough to grow in. The goal's OTA layout replaces
  that partition with slots of at least 2 MB; until it lands, anything else that grows the Wi-Fi
  image has to count these bytes.

  These are build outputs, not measurements of a device, and nothing here is timing evidence.
- NOT host-tested, and said so in the test's header and the Makefile: the binding and the
  manager's protocomm handshake. protocomm and network_provisioning both return early on
  ESP-IDF's linux target, so the real handshake runs first on the owner's bench (S9).

## ASSUMED values

- Three joins at boot before the access point returns (`CHORUS_PROVISION_BOOT_JOIN_ATTEMPTS`).
- 120 s between the access point's own retries of the stored network
  (`CHORUS_PROVISION_REJOIN_SECONDS`).
- In the binding: 20 s for one join, 1 s for a reply to reach the phone before the radio leaves
  the access point's channel, 2 attempts and 45 s for a join the manager owns.
- The HTTP server's defaults (1024-byte request headers, 4096-byte task stack) against a real
  phone's browser.
- The ESP32-S3's radio being 2.4 GHz only (bench packet S9; verify against the datasheet).
- The board itself and its 8 MB flash stay ASSUMED against the Needs item "Your ESP32-S3 boards:
  module markings and a read-only chip report".

## Deviations from the envelope

- `store.h` is the envelope's text through the pinned clang-format, which the gate requires: one
  line differs by spaces (`CHORUS_STORE_FAILED /* ... */`).
- Two states were added to the envelope's eight: `wired` and `refused`.
- The access point's name carries 6 random characters, not part of the endpoint id: the id does
  not exist yet when the access point first rises (identity loads after the link), and a random
  suffix identifies nothing in a pasted log.
- The retry count and the retry interval are constants in `provision.h`, not `endpoint.conf`
  keys: no shared file grows for values nobody has measured.
- The reset does not restart the speaker: a restart with the output stage live is the amplifier
  sequencer's to order. Erased now, unprovisioned at the next boot.
- `fake_radio` gained a network in range and an access point (its event cap went from 32 to 256).

## Not chosen

- BLE provisioning; protocomm security 2 (a salt and verifier must exist beforehand and the secret
  still needs to reach the phone); a proof of possession derived from the MAC or the id (the
  access point's name would broadcast the derivation's input); an open access point with no proof
  of possession (anyone in range could hand the speaker a network).
- Handing the form's network to the manager (`network_prov_mgr_configure_wifi_sta`): it stores
  the network in the Wi-Fi driver's namespace, a second home.
- Building protocomm for the host from Espressif's sources with stubs: owning their build is not
  worth a handshake Espressif's own target tests already cover.
- A captive-portal redirect, a QR code and a scanned-network list on the page: polish for the
  web app's goal.

## Follow-ups

- The owner's bench session S9 (the queue item is filed by the goal's coordinator).
- Provisioning runs after the amplifier's bring-up (the envelope's order), so a board whose
  amplifier does not answer never reaches it; S9 says so. Moving it earlier is a change to
  `app_main.c`'s order for a later goal to weigh.
- `esp_hal.c`'s `hal_radio_init` still erases NVS on `ESP_ERR_NVS_NO_FREE_PAGES`; with the store
  initialised first it is not reached in practice, but it should go, by whoever next edits it.
- The pairing button's long hold has no GPIO binding yet; `chorus_esp_provision_request_reset` is
  what it calls.
- The setup secret is on the console only; the endpoint console's `status` could repeat it, and
  goal 22's adoption screen is where the walk-through lives.
- The app partition's headroom after this change is in the table above; the OTA layout of the
  same goal changes the slot size, and the two have to be read together at integration.

## Revisit when

S9 answers; the owner's board is identified; Flash Encryption is ever allowed on a production
board (the stored passphrase would then be protected at rest); or `network_provisioning` 1.3.x has
aged enough to take.
