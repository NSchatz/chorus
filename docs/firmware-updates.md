# Firmware updates

How a speaker gets new firmware: an image is staged on the server, the server checks it, the
control state says which speakers it would update, and nothing is installed until somebody
sends the install command (K93, I13). The decisions are in
`docs/decisions/0110-explicit-firmware-installs.md`; the wire is `docs/protocol.md`, "Firmware
update"; the commands and the state are `docs/control-plane.md`, "Firmware: staged images and
explicit installs".

Nothing on this page is timing evidence, and nothing here has been run on a board: the
hardware demonstration (a real install and a real rollback) is a bench session of the owner's.

## Staging an image

The server is started with `--firmware-dir <dir>`. One image is two files in that directory:

- `<name>.bin`: the application image the build produced (`chorus-endpoint.bin`).
- `<name>.manifest`: four lines of `key = value` text: `version`, `board`, `size`, `sha256`.

`<name>` is what the install command names: 1 to 32 lower-case letters, digits and hyphens.

The helper writes both from a build directory, and touches local files only:

```
tools/firmware-stage.sh <build-dir> <firmware-dir> [name]
```

It reads the board profile the image was built for from the build directory, the version from
the image's own application description, and computes the size and the SHA-256. The same thing
by hand, for an image that is not in a build directory:

```
chorus-server stage-firmware --image <file.bin> --board <profile> --firmware-dir <dir> [--name <name>]
```

The server reads the directory when it starts and whenever it is sent `firmware_rescan`:

```
curl -s -H 'Content-Type: application/json' -d '{"v":2,"t":"firmware_rescan"}' http://<server>/api/command
```

Each image is listed in the state message's `firmware.images` with a verdict. `verified`
means: the name is an identifier, the manifest reads, the file's size and SHA-256 are the
manifest's, the file starts with the ESP image magic, and the version inside the image is the
manifest's. Anything else is `refused` with a `reason` by name (`digest-mismatch`,
`size-mismatch`, `no-manifest`, `bad-manifest`, `not-an-esp-image`, `version-mismatch`, ...).
A refused image is listed and is never offered to a speaker.

## What "update available" means

Each speaker that takes updates reports the version it runs, its board profile and its slot.
The state shows them under the speaker's `firmware`, with `update_available`: true when a
staged, verified image for that board carries a different version from the one the speaker
runs.

It is information. Staging an image, a speaker connecting, a speaker reconnecting and a
server restart send a speaker nothing. Versions are compared as names, not as numbers, so an
older image for the same board also shows as available: going back is an install like any
other.

## The install action

```
curl -s -H 'Content-Type: application/json' \
  -d '{"v":2,"t":"firmware_install","speaker":"<speaker id>","image":"<name>"}' \
  http://<server>/api/command
```

or, for every present speaker of the image's board that does not already run its version:

```
curl -s -H 'Content-Type: application/json' \
  -d '{"v":2,"t":"firmware_install","all":true,"image":"<name>"}' \
  http://<server>/api/command
```

The server refuses, by name and without sending anything, when the speaker is unknown or
absent, takes no updates, is busy with another install, is of another board, or already runs
that version (add `"force":true` to install the same version again), and when the image is
unknown, was refused, or changed on disk since it was verified.

The speaker's `firmware.state` then goes `requested`, `receiving` (with `received` of `size`
bytes), `verified` (written, digest good; the speaker reboots), `pending_verify` (the new
image is running on trial), `confirmed`. The image travels inside the speaker's own encrypted
session, a little at a time, so music keeps playing on the other speakers and on this one
until it reboots.

`firmware_cancel` with the speaker's id abandons a transfer that has not been verified yet.
An install is tied to the server process that started it: if the server stops in the middle,
the next one does not resume it, tells the speaker to drop what it holds, and shows
`interrupted`. Send the install command again.

## From the app

The app's speakers screen (`#/speakers`; `docs/app.md`, "Firmware: update available and the
explicit install") shows the same state and sends the same three commands. Each speaker that
reported what it runs shows its version, board and slot and its `firmware.state` in words, with
the `reason`; "Update available" appears for a speaker whose `update_available` is true, with
the version and the name of each staged image that is an update for it; the staged images are
listed below with their verdicts, beside a "Rescan" button (`firmware_rescan`).

"Install" is a button beside one image on one speaker. Pressing it asks, naming the image and
the speaker; "Yes, install it" sends `firmware_install` with that speaker and that image, and
nothing else in the app sends one: not opening the screen, not a new state, not a rescan. The
app sends neither `"all": true` nor `"force": true`, installs nothing by itself, and uploads no
image (staging is the files above). "Cancel install" is there while the state is `requested`
or `receiving` and sends `firmware_cancel`.

The app is one more client of the command, so the bench variable below applies to it exactly as
it does to `curl`: the server, not the app, decides whether a transfer may start. Pressing
"Install" for a speaker on the network, on a server whose environment does not hold
`CHORUS_OWNER_AT_BENCH=1`, is refused `owner-not-at-bench`, the speaker is sent nothing, and
the speaker's row shows the refusal in the server's words. The app cannot set the variable and
has no way round it. With the variable set by the owner's deploy, the install still starts only
when somebody presses "Install" and confirms.

The app's tests ran no install on a board: its unit test is over a scripted server, and its
live test (`web/live/firmware.live.js`) is a real server with a staged image and a scripted
speaker session on loopback that keeps the bytes in memory. Neither sets the bench variable.

## Rollback

A new image runs on trial. It confirms itself only after it has rejoined the server; an image
that does not manage that within its window marks itself invalid and reboots, and the
bootloader starts the previous image again. The speaker then reports it and the state shows
`rolled_back`, with `image` and `image_version` naming what was tried and `version` naming
what runs. Nothing retries it. `update_available` stays true, because the image is still
staged: remove or replace the image, then `firmware_rescan`.

There is no anti-rollback, no Secure Boot and no Flash Encryption in any chorus build, and no
eFuse is written by any of this (BRIEF.md section 3.1); going back to an older image is always
possible.

## The bench variable: installs on real speakers are the owner's

An install to a speaker on the network writes to a real device. The server refuses to start
a transfer to any address that is not the server's own host (`owner-not-at-bench`) unless its
environment holds `CHORUS_OWNER_AT_BENCH` set to `1`. Nothing in the repository sets it: no
script, no test, no container file. Tests and the emulator run reach the server on loopback,
where the guard does not apply.

The owner's deploy carries it. In the owner's homelab repo's own service definition for the chorus server
(which lives outside this repository), the owner adds the variable to the server's
environment, for example in a compose file:

```
environment:
  CHORUS_OWNER_AT_BENCH: "1"
```

or, for a run by hand, `CHORUS_OWNER_AT_BENCH=1 chorus-server --firmware-dir ... `. With it
set, the install commands above still have to be sent: the variable allows an install, it
never starts one. Without it, everything else on this page works (staging, verification,
`update_available`), and only the transfer is refused.

## Under the emulator

`make ota-qemu` (a gate step) runs an install end to end with no board: the real image of the
emulator's board profile (`qemu-s3-openeth`, ADR 0109), a real server, an explicit install of a
good image, which is confirmed, and of a bad one built never to confirm, which ESP-IDF's
bootloader rolls back. It is simulation, not timing evidence; the report is
`docs/measurements/ota-qemu-rollback.md` and the record
`docs/decisions/0111-ota-under-the-emulator.md`.
