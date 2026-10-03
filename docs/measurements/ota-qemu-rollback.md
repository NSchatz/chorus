# A firmware update under the emulator: a good image installs, a bad image rolls back

Source: simulation
Build measured: `7ebdfc18fef195e0f3f1f996911c1941c6652459`

Not timing evidence: an emulator's clock is not a crystal's, and nothing below says a board
works. The hardware demonstration is the owner's bench session.

What ran: `make ota-qemu` (`tools/ota-qemu-run.sh`, `docs/decisions/0111-ota-under-the-emulator.md`)
on 2026-10-03, on main at the commit above with the change that adds the run (its tools, the
never-confirm build hook and the recorded board profile; no firmware source under
`firmware/src` or `firmware/main/*.c` differs from that commit). The pinned emulator
`esp_develop_9.2.2_20260417` (`tools/qemu/pins.conf`), the board `qemu-s3-openeth`, three images
of it, a real `chorus-server` on loopback. The id and key the board made are the emulator's,
from its random source in that run.

## The result

```
pass the-board-is-adopted-and-reports-image-a: chorus-a89c400b4c14: v0.1.0-149-g699e119 qemu-s3-openeth slot=0 state=idle reason=none image=
pass the-staged-images-are-verified: [('bad', 'ota-qemu-bad', 'verified'), ('good', 'ota-qemu-good', 'verified')]
pass nothing-is-offered-while-nobody-asks: update_available=True, firmware offers in the server's log: 0
pass the-install-of-good-is-accepted: HTTP/1.1 200 OK
pass good-boots-on-slot-1-on-trial: running slot=1 state=pending-verify version=ota-qemu-good
pass good-is-confirmed: ota-qemu-good qemu-s3-openeth slot=1 state=confirmed reason=none image=ota-qemu-good
pass good-is-valid-after-a-power-cycle: running slot=1 state=valid version=ota-qemu-good
pass the-install-of-bad-is-accepted: HTTP/1.1 200 OK
pass bad-boots-on-slot-0-on-trial: running slot=0 state=pending-verify version=ota-qemu-bad
pass bad-does-not-confirm-and-gives-itself-up: this image did not confirm in time
pass the-bootloader-rolled-back-to-good: after the trial, the board reads otadata: running slot=1 state=valid version=ota-qemu-good
pass the-server-says-rolled-back: ota-qemu-good qemu-s3-openeth slot=1 state=rolled_back reason=not_confirmed image=ota-qemu-bad
pass nothing-crashed: 0 panic or abort lines on the two consoles: the rollback is the trial's, not a crash's
pass the-efuse-file-is-byte-identical: sha256 before 2054600a...ac09c8, after 2054600a...ac09c8
ota-qemu: good installed, bad rolled back: PASS
```

(The two HTTP answers are the state message in full; cut here.)

## The board's console

First power-on, A, then GOOD:

```
rst:0x1 (POWERON),boot:0x4 (SPI_FLASH_BOOT)
I (393) boot: Loaded app from partition at offset 0x20000
I (1409) chorus-ota: running slot=0 state=valid version=v0.1.0-149-g699e119
I (27059) chorus-ota: rebooting into the new image
rst:0xc (RTC_SW_CPU_RST),boot:0x4 (SPI_FLASH_BOOT)
I (14585) boot: Loaded app from partition at offset 0x320000
I (15797) chorus-ota: running slot=1 state=pending-verify version=ota-qemu-good
I (17017) chorus-ota: update unit state=pending-verify
```

Second power-on, GOOD, then BAD and the rollback:

```
rst:0x1 (POWERON),boot:0x4 (SPI_FLASH_BOOT)
I (323) boot: Loaded app from partition at offset 0x320000
I (1446) chorus-ota: running slot=1 state=valid version=ota-qemu-good
I (82486) chorus-ota: rebooting into the new image
rst:0xc (RTC_SW_CPU_RST),boot:0x4 (SPI_FLASH_BOOT)
I (42281) boot: Loaded app from partition at offset 0x20000
I (16542) chorus-ota: running slot=0 state=pending-verify version=ota-qemu-bad
I (17772) chorus-ota: update unit state=pending-verify
E (77772) chorus-ota: this image did not confirm in time; marking it invalid and rebooting into the previous one
rst:0xc (RTC_SW_CPU_RST),boot:0x4 (SPI_FLASH_BOOT)
I (9056) boot: Loaded app from partition at offset 0x320000
I (10188) chorus-ota: running slot=1 state=valid version=ota-qemu-good
I (11368) chorus-ota: update unit state=rolled-back
```

## The server's log

```
firmware offer speaker=chorus-a89c400b4c14 transfer=336273608 image=good version="ota-qemu-good" board="qemu-s3-openeth" size=1410752 chunk_bytes=1024
firmware verified speaker=chorus-a89c400b4c14 transfer=336273608 image=good received=1410752 reason=none
firmware pending_verify speaker=chorus-a89c400b4c14 transfer=336273608 version="ota-qemu-good" slot=1 reason=none image_version=""
firmware confirmed speaker=chorus-a89c400b4c14 transfer=336273608 version="ota-qemu-good" slot=1 reason=none image_version=""
firmware offer speaker=chorus-a89c400b4c14 transfer=336273609 image=bad version="ota-qemu-bad" board="qemu-s3-openeth" size=1410768 chunk_bytes=1024
firmware verified speaker=chorus-a89c400b4c14 transfer=336273609 image=bad received=1410768 reason=none
firmware pending_verify speaker=chorus-a89c400b4c14 transfer=336273609 version="ota-qemu-bad" slot=0 reason=none image_version=""
firmware rolled_back speaker=chorus-a89c400b4c14 transfer=336273609 version="ota-qemu-good" slot=1 reason=not_confirmed image_version="ota-qemu-bad"
```

The bracketed numbers on the console are the emulator's milliseconds since each reset and are
not durations of anything a board does.
