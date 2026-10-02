# 0110: firmware images are staged and verified by the server, shown as an update available, and sent to a speaker inside its own session only on an explicit install command, never to a real address without the owner at the bench

- Status: accepted (goal 14, 2026-10-02)
- Decided by: the goal (program section 18; K93, I13, section 0.7) inside the coordinator's
  goal-14 design envelope (section 3), track `chorus-g14/ota-server`; every default below not
  cited is ASSUMED
- Implemented in: `crates/server/src/firmware.rs` (new: staging and verification, the
  manifest, the staging helper, the guard, the sender), `crates/server/src/control.rs`
  (`firmware_through`, the commands' server half inside the commit, `firmware_reported`,
  `firmware_interrupted`), `session.rs` (the `firmware_status` handler, `pump_firmware`,
  `session_down`), `clients.rs` (the reader's look), `config.rs` (`--firmware-dir`), `main.rs`
  (`stage-firmware`, the wiring); `crates/control/src/firmware.rs` (new: `Image`,
  `SpeakerFirmware`, `Report`, the state's words), `catalog.rs` (`firmware_install`,
  `firmware_cancel`, `firmware_rescan`), `zones.rs` (`firmware_targets`, the refusals, the
  state's `firmware` member), `speakers.rs` (`SpeakerNow::firmware`, the speaker's `firmware`
  object); `fixtures/control/v2/` (5 command vectors, `state-firmware`, 14 refusals);
  `tools/firmware-stage.sh`; `tools/conventions/check-flash-tools-refuse.sh` (guard readers;
  the search repaired); `audio-path.conf`; `crates/server/tests/firmware_install.rs`,
  `crates/control/tests/firmware_v2.rs`. The contracts are `docs/control-plane.md` ("Firmware:
  staged images and explicit installs"), `docs/protocol.md` ("What the chorus server does"),
  `docs/firmware-updates.md` (the owner's page) and `docs/conventions.md` rule 20

## Context

ADR 0108 gave the endpoint an A/B update unit and the wire (`firmware_offer`,
`firmware_chunk`, `firmware_status`), with a test acting as the sender. Nothing on the server
staged an image, knew what a speaker ran, or sent one. K93 asks for explicit installs: the
server may know an image and say an update is available, and an image is installed only when
the owner says so, per speaker or for all. Program section 0.7 asks that an OTA push to a real
address refuse unless the owner's bench variable reads 1. The audio server's thread population
(`6 + 2N + M`) and its 100-repetition determinism run forbid a firmware thread or a long-held
control worker, and a control request is at most 16 KiB.

## What was read

All read 2026-10-02: `BRIEF.md` section 3.1; the program's sections 0.3, 0.4, 0.7, 0.8, 0.9
and 4; the goal-14 design envelope (sections 2, 3, 4 and "Tracks and file ownership"), the
code survey (sections 3, 5, 7, 8 and "Seams and risks"), and the ota-core report ("How
ota-server drives this"); ADRs 0016, 0018, 0062, 0081, 0088, 0106 and 0108; `docs/control-plane.md`,
`docs/protocol.md` ("Firmware update"), `docs/conventions.md` (rules 13, 15, 19, 20);
`crates/protocol/tests/firmware_session.rs` (the reference sender), `firmware/tests/main_session.c`
(the host endpoint's flags and lines); the code each "Implemented in" names, and
`crates/server/src/router.rs`, `stream.rs`, `crates/control/src/speakers.rs`,
`tools/conventions/check-flash-guard.sh`, `check-flash-tools-refuse.sh`, `check-identity.sh`.
ESP-IDF v6.1 (Apache-2.0) at the pinned tag in `/cache/esp/esp-idf-v6.1`:
`components/bootloader_support/include/esp_app_format.h:77` (`ESP_IMAGE_HEADER_MAGIC 0xE9`) and
`:111` (the image header is 24 bytes), `components/esp_app_format/include/esp_app_desc.h:21-30`
(`ESP_APP_DESC_MAGIC_WORD 0xABCD5432`, `version[32]` at offset 16), and
`docs/en/api-reference/system/app_image_format.rst:135` (the description sits at the fixed
offset `sizeof(esp_image_header_t) + sizeof(esp_image_segment_header_t)`). No GPL source and no
reciprocally licensed design file was opened. No number here comes from outside the
repository and ESP-IDF's tree, so nothing is cited by URL.

## Decision

1. **A staged image is two files and a verdict.** `--firmware-dir <dir>` (it needs
   `--control-listen`, refused by name otherwise) holds `<name>.bin` and `<name>.manifest`
   (`key = value`: exactly `version`, `board`, `size`, `sha256`, each once). The server reads
   the directory at start and on `firmware_rescan` and lists every image, sorted by name, at
   most 16 (ASSUMED), each `verified` or `refused` with a reason by name: the name is a
   catalog identifier; the manifest reads; the size is within the wire's 16 MiB and is the
   file's; the SHA-256 is the file's; the first byte is the ESP image magic `0xE9`; and the
   version in the image's application description is the manifest's (so the version offered is
   the version the speaker reports once it runs the image, and `update_available` goes false
   after an install). A refused image is listed and never offered. The image is read and its
   size and digest checked AGAIN at the install action: what is sent is what was verified, or
   the command is refused `image-not-verified` and nothing is sent.
2. **SHA-256 is the protocol crate's own** (`FirmwareOffer::digest_of`, over the `sha2` the
   workspace already pins), so no crate was added and `deny.toml` is untouched.
3. **The staging helper is the server's own code**: `chorus-server stage-firmware --image
   <file.bin> --board <profile> --firmware-dir <dir> [--name <name>]` writes the image, then the
   manifest by rename (a scan between the two sees an image with no manifest, refused, never a
   manifest describing half a file), takes the version from the image itself, and grades the
   result as the server will. `tools/firmware-stage.sh <build-dir> <firmware-dir> [name]` finds
   the board the build recorded and calls it. Local files only.
4. **What a speaker runs is a fact about now** (ADR 0018): `SpeakerNow::firmware`, from its
   own `firmware_status`, never persisted. It is written in the speaker's state object after
   `roles` only once the speaker has reported, so `state-speakers` did not move. The staged
   images are the state's last member, `firmware`, written only by a server with a firmware
   directory, so `state-empty` and `state-rich` did not move. `update_available` is derived at
   render time from the images (a verified image for the speaker's board with another
   version: versions are names, not numbers, so going back is an install like any other) and is
   information only.
5. **The speaker's `state` holds outcomes.** `idle`, `requested`, `receiving`, `verified`,
   `pending_verify` say what it is doing; `confirmed`, `rolled_back`, `refused`, `interrupted`,
   `cancelled` say how the last install this server process saw ended, and stay until the next
   install or a restart: the speaker says `rolled_back` once and `idle` after, and the owner
   still needs to see the rollback.
6. **Three commands in catalog v2** (the ADR 0081 and 0088 precedent: no version bump; at
   version 1 they are "not a command"): `firmware_install` {`speaker`, `image`} or {`all`:
   true, `image`}, optional `force`; `firmware_cancel` {`speaker`}; `firmware_rescan` {}.
   Every refusal names its field and starts its detail with its name: `unknown-image`,
   `image-not-verified`, `speaker-absent`, `not-updatable` (never reported a version),
   `busy`, `wrong-board`, `already-running`, `nothing-to-install`, `nothing-to-cancel`,
   `no-firmware-dir`, `owner-not-at-bench`, and the catalog's usual unknown-speaker words.
   **`force` is allowed** (envelope: "decide and record"): reinstalling the running version is
   the owner's recovery for a slot they no longer trust, and refusing it by default with a
   named override keeps the accident (an install that changes nothing) out of the way.
   **`all`** reaches every present speaker of the image's board that takes updates, is not
   busy and does not run the version (any version with `force`); speakers of another board are
   not reached, not refused; reaching nobody is refused.
7. **The model accepts, the server sends, in one commit.** `Zones::firmware_targets` says who
   an install reaches and refuses what it must; applying the command marks each target
   `requested` and nothing more. The server half runs inside the same `commit` as the model
   (ADR 0106's `speaker_forget` shape): `Firmware::start` checks every target's session, the
   guard and the file for every target before queuing anything for any, so a refusal leaves
   the state as it was and nothing on any wire.
8. **The sender has no thread of its own.** The offer is queued on the speaker's session queue
   by the control worker applying the command; every chunk is queued by that session's own
   READER thread, when a `firmware_status` arrives and every time it looks up from its socket
   (`read_requests_and_upstream`'s look, at least every 200 ms). The writer seals them like the
   audio beside them. `chunk_bytes` 1024 (ASSUMED) and a window of 16 chunks beyond the last
   acknowledgement (ASSUMED: one acknowledgement's worth; the wire allows 32), so at most 16 of
   the queue's 128 slots and 16 KiB on the socket are firmware; a full queue is never waited
   on. The population is `6 + 2N + M` as before (no `thread::spawn` in the new code) and no
   control worker is held past its command.
9. **A transfer lives as long as its session, and nothing is resumed.** An install is tied to
   the session it travels in: when that session ends before `verified`, the transfer is dropped
   and the speaker shows `interrupted`. A speaker that comes back `receiving` a transfer the
   server is not carrying (the session ended, or the server restarted: nothing about an
   install is persisted) is sent the cancel and shown `interrupted` with reason `not_resumed`;
   it abandons the transfer and says `idle`. The owner sends `firmware_install` again. Transfer
   ids start from a random value per process, so a new process never mistakes an old
   transfer for one of its own. Resuming within one process (the wire allows it) is a
   follow-up, not a need: the envelope's rule is about restarts, and "a new command" is the
   simpler promise.
10. **The guard** (program section 0.7): `Firmware::start` refuses a transfer to a session
    whose peer address is not loopback (an IPv4-mapped loopback counts as loopback) unless
    `owner_at_bench()` reads `CHORUS_OWNER_AT_BENCH` as exactly `1`, in the approved Rust
    form; the refusal is `owner-not-at-bench`. The decision is a pure function,
    `transfer_allowed(peer, at_bench)`, unit-tested with RFC 5737 and RFC 3849 addresses
    (`a_transfer_to_a_peer_that_is_not_loopback_is_refused_without_the_owner_at_the_bench`) and
    through `start_with` (one non-loopback target refuses an `all` whole). The variable is read
    at each install action, not at start. `docs/firmware-updates.md` says how the owner's
    deploy carries it; nothing else sets it.
11. **The flash-tools check learns guard readers, and its search is repaired.**
    `check-flash-tools-refuse.sh` part 1 required every guard reader to be a listed shell
    tool; the server is a program, not a tool the check can run with shims. It now also lists
    guard readers, each naming the test in it that grades its refusal (the check holds the file
    to holding that test and reading the guard; `make gate`'s test step runs it); a reader that
    calls a flashing program must still be a listed tool. Wiring that in showed the part-1
    search never ran: it was handed to `xargs command grep`, `command` is a shell builtin that
    `xargs` cannot run, and the error went to `/dev/null`, so the list was always empty. The
    search now runs `grep` itself and is checked to find at least the listed tool; with it
    running, the endpoint scanner (`firmware/check/endpoint_scan.c`, which names the eFuse
    tool in its refusal list) is excluded beside the conventions checks. Net: the check is
    stricter than it was. The other flash-guard check and its fixtures are untouched.

## The end-to-end tests

`crates/server/tests/firmware_install.rs` runs the real `chorus-server` and the real C endpoint
(`chorus-endpoint-session --ota-flash`, the board's session and update code over a file-backed
two-slot fake flash; the images from its `--ota-make-image`, staged with `stage-firmware`).
Every wait is on an event (an endpoint line, a server log line, a state member) with a 90 s
deadline; none is a sleep that hopes.

- `nothing_installs_until_the_explicit_install_action` (THE test of line C): staged, verified,
  newer; the window (twenty state reads, a `firmware_rescan`, a server kill and restart, the
  endpoint's own reconnect) shows `update_available` and `idle`, zero `firmware offer` lines in
  both server processes, only `idle` from the endpoint and its flash file byte for byte; then
  `firmware_install`: written and verified (the endpoint's lines), the image in the flash byte
  for byte, the binary restarted on the same flash boots the new slot on trial and confirms, and
  the state shows 2.0.0, `confirmed`, no update available; one offer.
- `a_bad_digest_is_refused_before_activation_and_never_offered`, `an_image_that_never_confirms_is_rolled_back_and_the_state_says_so`,
  `firmware_install_with_all_reaches_every_speaker_of_the_board_and_no_other`,
  `a_wrong_board_image_and_every_other_refusal_is_named_and_offers_nothing`,
  `an_install_in_progress_when_the_server_stops_is_not_resumed_after_restart` (the endpoint is
  held with SIGSTOP across the command and the kill, so the install is in progress by
  construction; whether the offer reached its socket first depends on the audio queued ahead
  of it, so both cases are graded by the same rule and the one that happened is printed; the
  orphan-cancel path is also graded alone in the unit tests).

## Deviations from the envelope

- The per-speaker object carries `reason`, `image`, `image_version`, `received` and `size`
  beside the envelope's fields (the transfer progress, named), and the images carry `reason`
  when refused.
- Beyond the envelope's checks, verification also requires the application description's
  version to equal the manifest's (decision 1), and the install action re-reads the file.
- `tools/firmware-stage.sh` and a `chorus-server stage-firmware` subcommand: both, the script
  calling the subcommand.
- The sender's window is 16, not the wire's 32 (decision 8).
- A transfer is not resumed even within one process after its session drops (decision 9).
- `check-flash-tools-refuse.sh` is changed (decision 11); the envelope did not foresee a Rust
  guard reader meeting part 1.

## ASSUMED

- `chunk_bytes` 1024; a window of 16 chunks; at most 16 staged images listed.

## Follow-ups

- The page and the app: show `update_available`, the install action and the progress (goal 22).
- Resuming a transfer within one server process after a dropped session.
- Home Assistant entities for `update_available` and the install (the HA goal).
- The owner's bench session: a real install and a real rollback on the board (S9, FLEET-10's
  evidence), with the bench variable set in the owner's deploy.
