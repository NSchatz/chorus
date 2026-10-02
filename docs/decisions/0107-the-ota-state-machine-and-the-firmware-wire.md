# 0107: the endpoint's firmware update is a pure A/B state machine graded over a fake flash that models ESP-IDF's bootloader, an image travels inside the session as three new messages, and ESP-IDF's OTA calls live in one glue unit

- Status: accepted (goal 14, 2026-10-02)
- Decided by: the goal-14 design envelope, section 2 (OTA on the endpoint) and "The wire", track
  `chorus-g14/ota-core` (PR #107); the program's research
  `.claude/goals/2026-09-chorus-research/research-platform-network.md` section 3 (OTA testability
  without hardware: "the A/B decision logic must be chorus's own pure C"); BRIEF.md 5.9 and
  section 3.1 rule 2 (no eFuse burns on dev hardware); the owner's decisions K91, K92, K93 and I13
  (rollback, explicit installs); the placements below where the envelope left them open
- Implemented in: `firmware/include/chorus/ota.h`, `firmware/src/ota.c` (the state machine);
  `firmware/tests/fake_flash.{c,h}` (two slots, otadata, the bootloader model, fault injection,
  the note's fake), `firmware/tests/test_ota.c` (`make firmware-check` target `ota`);
  `firmware/main/esp_ota.{c,h}` (the binding), `firmware/main/app_main.c` (one report call, the
  unit handed to the session), `firmware/partitions.csv`, `firmware/sdkconfig.defaults`;
  `crates/protocol/src/v2/{catalog,messages,codec}.rs` and
  `firmware/include/chorus/protocol_v2.h`, `firmware/src/protocol_v2.c` (`0x18` to `0x1A`,
  `features` bit 1), `fixtures/protocol/v2/firmware_*`, `capabilities_ota` and five rejection
  vectors; `firmware/src/session.c` (`chorus_session_config_t.ota`),
  `firmware/tests/main_session.c` (`--ota-flash` and its siblings);
  `crates/protocol/tests/firmware_session.rs` (Rust against the real C endpoint on loopback);
  `firmware/check/endpoint_scan.c` (`OTA_GLUE_UNIT`), `firmware/tests/test_scan.c`,
  `firmware/endpoint-units.conf`, `tools/firmware-image-guard.sh` (`grade_rollback`),
  `tools/firmware-image.sh`, `tools/gate.sh` (`profiles_all_built`);
  `firmware/config/endpoint.conf` (`ota_confirm_seconds`). The contract is `docs/protocol.md`
  ("Firmware update")

## Context

Until this change the endpoint tree could not update itself and was forbidden to try: the source
scan refused every ESP-IDF OTA call everywhere, the image was one 1.5 MB factory app in the
default 2 MB of flash, and the brief's fleet section (5.9) says OTA "must exist before the fleet
grows past two devices". Goal 14's foundation line is "The OTA state machine survives every
injected fault in make gate and rolls back a bad image", and its safety line is "The safety
scans pass for every target". There is no board on a bench (the owner's ESP32-S3 is unidentified;
its flash size is ASSUMED), ESP-IDF's host target does not simulate app update or the bootloader's
rollback (the research file, section 3.2), and guardrail 2 forbids the one thing ESP-IDF's
anti-rollback does. So the decisions had to be code a host can grade, the bootloader had to be
modelled from its source, and the binding had to be small enough to read.

## What was read

All on 2026-10-02. ESP-IDF v6.1 at the pinned commit `fff9895c82d744c7237be8847347bdd1b07c6643`
(`/cache/esp/esp-idf-v6.1`, Apache-2.0, permissive: its source may be read and cited,
`docs/clean-room.md`):

- `components/bootloader_support/include/esp_flash_partitions.h:67-82`: the image states
  `ESP_OTA_IMG_NEW` 0, `PENDING_VERIFY` 1, `VALID` 2, `INVALID` 3, `ABORTED` 4, `UNDEFINED`
  all-ones, and the otadata entry (sequence, label, state, crc).
- `components/bootloader_support/src/bootloader_utility.c`: `write_otadata` erases the sector and
  then writes it (:307-318); `bootloader_utility_get_selected_boot_partition` (:376-474) marks a
  `PENDING_VERIFY` entry `ABORTED` at boot (:393-401), boots slot 0 when no entry is usable and
  there is no factory app (:404-420), else takes the active entry's slot as (sequence - 1) mod the
  slot count and turns `NEW` into `PENDING_VERIFY` (:433-449); `set_actual_ota_seq` writes entry 0
  as `VALID` on the first boot of blank otadata (:488-499); `bootloader_utility_load_boot_image`
  verifies the selected image and works backwards and then forwards through the other slots when
  it does not load (:576-627).
- `components/bootloader_support/src/bootloader_common_loader.c`: the entry's CRC (:73-76), an
  entry is invalid when erased, `INVALID` or `ABORTED` (:78-81) and valid when not invalid and its
  CRC holds (:83-86), the active entry (:88-97), the higher sequence of two valid ones (:151-175).
- `components/app_update/esp_ota_ops.c`: `esp_ota_begin` refuses the running partition and a
  running image still `PENDING_VERIFY`, erases, and drops the inactive otadata entry (:155-230,
  :1316-1362); `esp_ota_end` verifies the written image (:617-663); `rewrite_ota_seq` is erase
  then write (:665-680); the sequence for a target slot (:696-744); `esp_rewrite_ota_data` writes
  the entry `NEW` (:778-812); `esp_ota_set_boot_partition` verifies first (:849-860); rollback is
  possible only when the other entry is valid and its image verifies (:1077-1128); confirm and
  invalidate rewrite the active entry, the latter returning `ESP_ERR_OTA_ROLLBACK_FAILED` when
  there is nothing to go back to (:1179-1236); `esp_ota_get_state_partition` (:1282-1312).
- `components/bootloader/Kconfig.app_rollback:3-21`: `BOOTLOADER_APP_ROLLBACK_ENABLE` ("If during
  the first boot a new app the power goes out or the WDT works, then roll back will happen") and
  `BOOTLOADER_APP_ANTI_ROLLBACK`, which depends on it.
- `docs/en/api-reference/system/ota.rst:36-110` (app rollback and its states),
  `docs/en/api-reference/system/app_image_format.rst:100-130` and
  `components/bootloader_support/include/esp_app_format.h:77-121` (the image: header, segments,
  the checksum byte on a sixteen byte boundary, the appended SHA-256),
  `components/bootloader_support/src/esp_image_format.c:59,1045-1066` (the checksum),
  `components/esp_app_format/include/esp_app_desc.h:21-42` (the application description),
  `docs/en/api-guides/partition-tables.rst` (offsets, the otadata size).
- In this repository: the goal-14 survey and design envelope; `firmware/check/endpoint_scan.c`,
  `tools/firmware-image-guard.sh`, `tools/firmware-flash.sh`, `firmware/check/efuse-kconfig.list`;
  `firmware/src/session.c`; `crates/protocol/src/v2/`; ADR 0015 (fakes with a log the unit cannot
  reach), ADR 0039 (the session), ADR 0044 (the 1.5 MB table this retires), ADR 0057 (profiles).

No GPL source was opened. QEMU is not used by this change.

## Decision

1. **The decisions are one pure unit.** `chorus_ota_t` (`firmware/src/ota.c`) reads no clock, holds
   no heap and names no ESP-IDF call. It is handed the medium as `chorus_ota_flash_t` (running
   slot, slot state, slot capacity, begin, write, finish, abandon, set boot, confirm, invalidate
   and reboot, reboot), a note store as `chorus_ota_notes_t`, and the time as monotonic
   nanoseconds. Its states are the envelope's: running-valid, receiving, written-unverified,
   pending-reboot, pending-verify, valid, rolled-back, refused.
2. **The five rules** are in `chorus/ota.h` and are what the test grades: nothing is written
   without an offer; the boot slot changes only after the whole image is written AND the SHA-256
   of the bytes written equals the offer's AND the medium's own check passes; a new image is on
   trial until its session reached its server within `ota_confirm_seconds`, else it marks itself
   invalid and reboots; a rollback is reported once, naming the image; every fault leaves a
   bootable VALID slot selected.
3. **The digest is of what arrived, the medium checks what landed.** The unit hashes the bytes in
   order as it writes them (PSA SHA-256, the call `noise.c` uses), and the medium's `finish`
   (`esp_ota_end`: the image's own checksum and appended hash, read back from flash) covers a
   flash bit that did not take. Both must hold before the selection.
4. **The self-test is the session.** An image on trial confirms when the first record from the
   server opens after the handshake: by then the link, the key exchange, the server's pin and the
   endpoint's adoption all held. It is the cheapest test that fails for the faults a bad network
   image actually has, and it needs no new message.
5. **The note** (`ota1 <transfer> <slot> <version>`, at most 128 bytes, NVS key `ota_note`) is
   written BEFORE the boot selection, so a trial that ends in a rollback is always known to have
   happened. At boot a note whose slot reads `INVALID` or `ABORTED` is a rollback; one whose slot
   has no record (power went before the selection landed) describes nothing and is cleared; one
   about the running, confirmed slot is cleared.
6. **The fake models the source, not the documentation's summary.** `fake_flash.c` keeps two
   otadata entries rewritten by erase-then-write, selects as the bootloader does, verifies images
   in ESP-IDF's real image format (so a real image verifies and a truncated one does not), and
   counts every change to the medium as a step at which power can be lost, the bootloader's own
   writes included. The note's fake goes dead with the board's power, as the unit would.
7. **The wire** is three messages in the session-control block, inside records: `0x18
   firmware_offer`, `0x19 firmware_chunk`, `0x1A firmware_status`, and `capabilities.features`
   bit 1 `ota`. Field by field in `docs/protocol.md`. Chunks are written only in order; a
   duplicate is ignored, a gap is answered once with the resume point; a status follows every 16
   chunks and every change. A second, different offer mid-download is refused `busy` and the
   transfer in progress is untouched; the same offer again resumes; the cancel abandons.
8. **One glue unit.** `firmware/main/esp_ota.c` is the only unit the source scan allows ESP-IDF's
   OTA calls in. It is named in the scanner (`OTA_GLUE_UNIT`), not in `endpoint-units.conf`, so
   the list cannot widen the rule. `esp_https_ota` and `esp_http_client` are refused in every
   unit, the glue included: an image arrives only inside the session.
9. **The layout**: `firmware/partitions.csv` with nvs, otadata, phy_init and two 3 MiB app slots,
   no factory app, ending at 0x620000; `CONFIG_ESPTOOLPY_FLASHSIZE_8MB`;
   `CONFIG_BOOTLOADER_APP_ROLLBACK_ENABLE=y`; `# CONFIG_BOOTLOADER_APP_ANTI_ROLLBACK is not set`.
   `CHORUS_MIN_FLASH_MB` moves from 2 to 8 and the single-app table of ADR 0044 is retired.
10. **The safety scans say so per target.** The image guard asserts, in the generated sdkconfig
    and both `sdkconfig.json`, that rollback is on and anti-rollback is off (with red
    demonstrations for each way that can be false); `tools/firmware-image.sh` prints `safety scan:
    <profile>: no eFuse write, no Secure Boot, no Flash Encryption, no anti-rollback: pass` after
    the guard passes; the gate's `firmware-profiles` step fails unless the profiles built and
    scanned in the run are exactly `firmware/boards/*.conf`.

## Evidence

Host, 2026-10-02, this branch. Not timing evidence: every time in these tests is a number the
test passes in.

- `make -f firmware/Makefile ota`: `test_ota: 118 checks, 0 failed` and `ota faults: 3297
  injected, 3297 survived (a VALID slot bootable after each), bad image rolled back`. Power is
  lost at every medium step (30 or 31 of them) of nine update scenarios: a good image, one that
  never confirms and one that panics, each onto a freshly flashed board, a board updated once and
  a board that rolled a bad image back. The seeded campaign is 3000 runs over 18 fault kinds.
- The test is not vacuous. Seven deliberately broken state machines were each built and run
  against it and each turned it red: the digest not compared (1 check failed), the medium's check
  skipped (20), the deadline never invalidating (10), an offer during the trial accepted (3), a
  chunk past a gap written (4), the note never written (3), the slot selected before the digest
  (41). A flash with no image is reported `nothing bootable` by the model, so the invariant can
  be false.
- `cargo test -p chorus-protocol --test firmware_session`: 3 tests. The Rust messages against
  the real C endpoint binary on loopback: a wrong-board offer and a corrupted image refused, a
  good image installed in a window, the reboot, the trial, the confirm, a bad image installed,
  never confirming, rolled back and reported; a transfer surviving a dropped session; an
  endpoint without the unit not offering the feature.
- `firmware/build/test_protocol_v2`: 391 checks, 0 failed (the new vectors and rejection vectors
  both ways in C); `cargo test -p chorus-protocol`: all suites pass, with one new rules test.
- `firmware/build/test_scan`: 105 checks, 0 failed (four red OTA demonstrations outside the glue,
  two red transport demonstrations, one green for the glue).
- The image: `brick-s3-wired` builds under ESP-IDF v6.1 with the layout, `chorus-endpoint.bin`
  0x152e00 bytes in a 0x300000 slot (56% free), and passes the image guard: no eFuse writer is
  linked by `app_update` with anti-rollback off, so no allowlist was needed.

## ASSUMED values

- 8 MB of flash and the 3 MiB slots: the owner's board is unidentified (the item "Your ESP32-S3
  boards: module markings and a read-only chip report").
- `ota_confirm_seconds = 60`: nothing measured how long the link and the first session take.
- `firmware_chunk` at most 4096 bytes, one status per 16 chunks (`CHORUS_OTA_ACK_EVERY`,
  `FIRMWARE_ACK_EVERY`), a sender's window of 32 chunks (`FIRMWARE_WINDOW_CHUNKS`): nothing
  measured what a session carrying audio tolerates.
- The backstop's 30 s margin after the unit's own deadline (`firmware/main/esp_ota.c`).
- `firmware_offer.size` at most 16 MiB and the 47-byte bound on version and board texts: sanity
  bounds on the wire, chosen, not derived.

## Deviations from the envelope

- `firmware_status` gains a trailing `image_version` (short text): the envelope's `version` is
  the RUNNING image's, and rule 4 asks the rollback report to name the image that was tried.
- The ops struct has `slot_capacity` and `abandon` beside the envelope's list, and no `sha256`
  function pointer: the unit calls PSA itself, as `noise.c` does.
- The note is its own two-function seam (`chorus_ota_notes_t`) rather than the shared
  `chorus_store_t`, and `esp_ota.c` reads and writes NVS key `ota_note` in namespace `chorus`
  directly after an idempotent `nvs_flash_init`. The store seam is another track's file, not
  merged when this was written; the key and namespace are the envelope's, so moving the glue onto
  `chorus_esp_store()` later changes no stored byte.
- `app_main.c` has two additions, not one call: `chorus_esp_ota_boot_report(confirm_seconds)`
  where the envelope places it, and `session->ota = chorus_esp_ota_unit(...)` where the session
  is configured. The report takes the trial's length because it also arms a backstop task in the
  glue: an image on trial that is still unconfirmed 30 s (ASSUMED) after the unit's own deadline
  is marked invalid and rebooted whether or not a session ever started. Without it an image whose
  bring-up fails before the session task exists (app_main returns early on a dead link or
  amplifier) would stay unconfirmed until its power was cycled. The backstop is binding code:
  claimed on the emulator and the bench, not on a host.
- The host binary gains `--ota-confirm-seconds` (a test cannot wait 60 s for a rollback) and
  `--ota-make-image` (so a test needs no image builder of its own). Its running version is read
  from the running slot's application description; `--ota-version` names only the first image.
- "An offer that names the running slot" cannot occur on this wire (an offer names no slot): the
  unit's target is always the other slot, and the medium's refusal of the running slot is tested
  on the fake directly.
- The scanner also refuses `esp_http_client` and the header `esp_ota_ops.h` outside the glue.

## Not chosen

- ESP-IDF's `esp_https_ota`. A second transport and a second trust decision (a TLS certificate)
  beside the session the endpoint already authenticated, and a URL an endpoint can be pointed at.
- Verifying the digest by reading the slot back. It blocks the session task for the length of a
  2 MB read; the running hash plus the medium's own read-back check covers the same faults.
- Confirming at boot, or after a fixed time. An image that boots and cannot reach its server is
  exactly the bad image a wall-mounted speaker cannot recover from by hand.
- Replacing a transfer in progress when a different offer arrives. A stale offer could then erase
  a nearly complete image; `busy` plus an explicit cancel keeps the decision with the server.
- An allowlist in the image guard for linked eFuse symbols. None links; had one, the answer was
  the Kconfig that drops it.
- Anti-rollback, even virtually. It is refused in every configuration this tree builds.

## Follow-ups

- The server's sender, staging and the explicit install action (track `ota-server`), on the Rust
  types here; `crates/protocol/tests/firmware_session.rs` is the reference sender.
- The emulator run (track `ota-qemu`): the glue is claimed there and on the owner's bench, not on
  a host. Its bad image is built with `-DCHORUS_OTA_NEVER_CONFIRM` (read by `esp_ota.c`), and its
  images need distinct `PROJECT_VER` values: the version the endpoint reports is the application
  description's.
- Move the note onto `chorus_esp_store()` once the store seam is merged.
- The owner's bench session: a real update and a real rollback on the board, and the board's
  flash size.
- `README.md`'s status table still says FLEET-10 is not started; the goal's ledger updates it.

## Revisit when

- The board is identified and its flash is not 8 MB.
- A bench shows the first session after boot taking a meaningful part of 60 s.
- An update needs to survive a reboot mid-download (today the unit restarts clean and the
  transfer starts again from byte 0).
