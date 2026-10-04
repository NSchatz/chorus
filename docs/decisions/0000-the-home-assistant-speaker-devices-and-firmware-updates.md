# 0000: every adopted speaker is a Home Assistant device linked to its room, and a speaker that reports its firmware has an update entity whose install action is the only sender of `firmware_install`, offering only a verified image above the running version

- Status: accepted, 2026-10-04. Extends 0138 (the Home Assistant integration), which left
  speaker devices and firmware to this goal; uses 0110 (explicit firmware installs) as it is.
- Decided by: the owner for what there is (K93, I13: nothing installs without an explicit
  install action; a speaker device with a firmware update entity); this record for the device
  model, what is offered, what is shown and how a refusal is mapped.
- Recorded by: the owner's agent harness, in the pull request that adds this record.
- Implemented in: `integrations/homeassistant/custom_components/chorus/` (`update.py`,
  `entity.py`, `coordinator.py`, `_aiochorus/models.py`, `_aiochorus/commands.py`,
  `_aiochorus/errors.py`, `strings.json`, `icons.json`, `quality_scale.yaml`),
  `integrations/homeassistant/tests/test_update.py`, `tests/fake_server.py`,
  `integrations/homeassistant/README.md`, `docs/home-assistant.md`

## Context

The catalog already has everything: the state's `speakers[]`, `speakers[].firmware` and
`firmware.images`, and the `firmware_install` command with its refusals by name
(`docs/control-plane.md`, "Firmware: staged images and explicit installs"). No server change
was needed and none is made. Home Assistant's `update` platform has an install action a
person presses, which is exactly the explicit action K93 asks for, and it also has state that
arrives by push, which must never become an install.

## What was read

All on 2026-10-04. `docs/control-plane.md` (the firmware section and the state rows),
`docs/firmware-updates.md`, `docs/decisions/0110-explicit-firmware-installs.md`, the vectors
`fixtures/control/v2/firmware_install.json`, `state-firmware.json`, `state-speakers.json` and
the `error-firmware-install-*` refusals, and the wording of the `owner-not-at-bench` refusal
in `crates/server/src/firmware.rs`. Home Assistant core 2026.9.3 (Apache-2.0), from the
installed package: `components/update/__init__.py` (the install service's checks, `state`,
`version_is_newer`, the progress attributes) and `helpers/device_registry.py`
(`async_get_or_create`, `via_device_id`, `async_get_device_by_identifier`). No GPL source was
opened.

## Decision

**A device per adopted speaker, made from the state and not from an entity.** The coordinator
creates it (`sync_devices`), so a speaker that has no entity is a device all the same. Its
identifier is `<server id>:speaker:<speaker id>`, its name the speaker's, its model `Speaker`
with the board profile as the model id and the firmware version as the software version once
reported. It is linked `via` its room's device while it has a room, and the server's device
otherwise; the room's device is under the server, so every speaker is under the server. The
coordinator makes the room's device first (the same device the room's entities attach to), so
the link never depends on which platform was set up first. The link follows the state: a
speaker moved to another room, or unassigned, is relinked; a forgotten speaker's device and
entity are removed; a renamed speaker renames its device. The room's name is the suggested
area when the device is first made; areas stay Home Assistant's.

**An update entity only for a speaker that has reported.** `speakers[].firmware` is written
only once a speaker said what it runs. Until then there is no entity, not an unknown one: an
entity that cannot say what is installed would be an install button with nothing behind it.

**One sender.** `commands.firmware_install` is called from `update.py`'s `async_install` and
nowhere else, and Home Assistant calls that method only for `update.install`. The entity has
`INSTALL` and `PROGRESS` and no `SPECIFIC_VERSION`, so the action cannot be given a version;
the command names one speaker and one image and never writes `all` or `force`. There is no
"install all" service, no button, and no code path from a state message to a command.

**What is offered: a verified image for the board, above the running version.** The server's
`update_available` is true for any verified image of another version, an older one included,
because on the server going back is an install like any other. In Home Assistant an update
that is "on" invites one click, and after every install the previous image would be offered
as the next update while it stays staged. So the integration offers an image only when its
verdict is `verified`, its board is the speaker's, its version sorts above the running one
(runs of digits as numbers, the rest as text) and the server says `update_available`; of
several, the highest version, then the first name. Going back, reinstalling, cancelling and
rescanning stay the server's commands. With nothing to offer, the latest version is the
installed one, which is how an update entity says "up to date". A refused image, one with no
verdict and one with a verdict this client does not know are never the latest version.

**What is shown.** `in_progress` from `requested` to `pending_verify`; a percentage from
`received` and `size` while `requested` or `receiving`, none while the speaker verifies,
reboots and runs on trial; and the attributes `install_state`, `reason`, `image`,
`image_version` and `board`, each with a translated name and the states translated, so an
outcome (`confirmed`, `rolled_back`, `refused`, `interrupted`, `cancelled`) is read on the
entity until the next install replaces it. Sensors for the same are goal 19's diagnostics.

**Refusals.** 0138 maps a refusal by its `field` and never matches `detail`. The firmware
refusals share two fields (`speaker`, `image`) and the catalog gives each a name that starts
the detail, followed by a colon and a space: that name is declared contract
(`docs/control-plane.md`), so `ChorusCommandError.name` reads it and `owner-not-at-bench`,
`busy` and `image-not-verified` each get their own message; the words after the name are
passed through and never matched. Every other one falls back to its field (`refused_speaker`,
`refused_image`).

## Consequences

- Home Assistant cannot go back to an older image; `docs/firmware-updates.md` says how.
- After a rollback the entity stays "on": the image is still staged. Nothing retries it.
- An automation that calls `update.install` installs. That is an explicit action of the
  person who wrote it, and the server's bench guard still applies to it.
- The fake server grew `firmware_install` with the server's refusals by name; what the real
  server does is still held by the server's own tests.
