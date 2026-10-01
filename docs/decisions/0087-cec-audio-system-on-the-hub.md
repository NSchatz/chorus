# 0087: the Linux hub is the TV's Audio System over the kernel's CEC API, its own codec and role in a new crate, TV keys going to the server as controller commands and the TV's power deciding the TV input's signal

- Status: accepted (goal 13, 2026-10-01)
- Decided by: the goal-13 design envelope, section 3 (CEC); program §17 and P2 Option A (stereo
  LPCM only, so the hub never advertises an encoded format); K81 (TV autoplay); K65 (the
  controller role); BRIEF.md guardrail 1 and K33 (clean room: never libcec, never GPL source);
  the placements below where the envelope left them open
- Implemented in: `crates/cec` (`codec.rs`, `validate.rs`, `role.rs`, `power.rs`, `adapter.rs`,
  `kernel.rs`, `fake.rs`, `driver.rs`); `fixtures/cec/` (36 vectors, Rust-only by declaration);
  `crates/client-linux/src/cec.rs`, `config.rs` (`--cec`, `--cec-arc`,
  `--cec-autoplay-on-power`, `--cec-osd-name`), `main.rs`, and the source role's offer
  (`source.rs`, `SourceSetup::tv_power`); `audio-path.conf`,
  `tools/conventions/check-rust-lints.sh` (the one unsafe module),
  `tools/conventions/check-shared-fixtures.sh` and `docs/conventions.md` (the declaration);
  `docs/cec.md`, `docs/linux-endpoint.md`, `fixtures/README.md`. Held by
  `crates/cec/tests/audio_system.rs` (27), `crates/cec/tests/fixtures.rs` (2), the crate's unit
  tests (15), `crates/client-linux/src/cec.rs` (2) and `config.rs` (1) unit tests, and
  `crates/server/tests/cec_tv.rs` (1, the real server binary)

## Context

The TV path (goal 13) puts the TV's sound on a Linux hub (optical or an ARC extractor into its
`--line-in`). A soundbar is controlled by the TV's own remote over HDMI-CEC, and the TV's power
turns it on and off; the chorus hub has to be the same thing to the TV, or the room is a
second remote away from usable. Nothing in chorus spoke CEC (the goal-13 survey). The owner's TV
models are unknown, so every TV-dependent behaviour is a parameter or a fake, and the real one
is the Needs item "The three TVs: model, eARC port, optical out and audio menu".

## What was read

All read 2026-10-01. Goal 13's research `cec.md` (the coordinator's) and everything it cites:
the kernel's CEC userspace API documentation prose (open, G_CAPS, G/S_PHYS_ADDR, G/S_LOG_ADDRS,
G/S_MODE, TRANSMIT/RECEIVE, DQEVENT, poll; the admin guide's CEC page and vivid page), the MIT
`cec_linux` 0.2.2 crate's `src/sys.rs` (ioctl directions and numbers, `VENDOR_ID_NONE`,
`PHYS_ADDR_INVALID`), and Android's Apache-2.0 HDMI service: `HdmiCecMessageValidator.java`,
`HdmiCecLocalDeviceAudioSystem.java`, `HdmiCecLocalDevice.java`, `HdmiCecController.java`,
`HdmiCecMessageBuilder.java`, `HdmiControlService.java` (`handleCecCommand`), `Constants.java`,
and the CTS host tests `HdmiCecSystemAudioModeTest`, `HdmiCecInvalidMessagesTest`,
`HdmiCecAudioReturnChannelControlTest`, `HdmiCecRemoteControlPassThroughTest`. In the repo:
`crates/client-linux/src/{front_panel.rs,source.rs,main.rs,config.rs,session.rs}`,
`crates/server/src/{controller.rs,session.rs,control.rs}`, the front-panel and autoplay
end-to-end tests, `audio-path.conf`, the conventions checks. Not opened (clean room): libcec,
v4l-utils (`cec-ctl`, `cec-follower`, `cec-compliance`), the kernel's C source and uapi headers,
the kernel's "cec-header" page, vivid's source.

## Decision

1. **A new pure crate, `crates/cec`**, no dependencies. The codec (header, named opcodes,
   operands; `fixtures/cec/` golden vectors written by a script independent of the Rust code);
   a validator restating Android's per-opcode rules (valid initiators, direct or broadcast,
   operand checks) and Android's handling of each outcome; the Audio System role as a pure state
   machine handed messages, the room's state and a monotonic millisecond count, returning
   effects; the `Adapter` trait; a `Driver` that runs the role on an adapter one turn at a time.
2. **The kernel adapter is chorus's own ioctl binding** (`kernel.rs`), libc's `ioctl` and `poll`
   declared `extern "C"` (no crate). Structs typed from the documented layouts with
   compile-time size asserts (76, 92, 56, 80 bytes) and every request number asserted equal to
   its asm-generic encoding from those sizes; `features` declared in the documented `[4][12]`
   shape, not the crate's transposed one. Mode `INITIATOR | EXCL_FOLLOWER_PASSTHRU` so the role,
   not the kernel's core, answers every message; one Audio System logical address, CEC 1.4.
   The only `unsafe` in the crate, approved in `check-rust-lints.sh` by this record.
3. **What is answered** is `docs/cec.md`'s table, each row tested against its CTS case. The
   Short Audio Descriptor is LPCM 2 ch 48 kHz 16 bit only (`09 04 01`, bytes built as Android
   builds them); AC-3, E-AC-3, DTS or any other request is Feature Aborted [Invalid operand]
   (P2 Option A). ARC messages are answered only with `--cec-arc` (off, ASSUMED).
4. **TV keys go to the server, never around it.** Volume Up and Down become `controller_command`
   `volume_step` +-2 points per press and per repeat; Mute becomes `mute_set` to the opposite of
   the room's mute as last reported. The hub declares the controller role; the server applies
   them to the hub's room through `controller::translate` and `Zones::apply`, clamped by the
   room's limit and quiet hours like every other path (I10). The server's `controller_state`
   (0x33) is what Report Audio Status says, and a change within 2 s of a TV volume key is pushed
   to the TV unprompted while System Audio Mode is on.
5. **The TV's power is a shared atomic** (`TvPower`) the CEC thread writes and the source role
   reads once per captured chunk through `TvSignal`: offered signal = audio present OR (TV on
   AND `--cec-autoplay-on-power`); a standby ends it at once and holds it ended until the audio
   detector has let go and found audio again or the TV is seen on again. Only an `optical` or
   `hdmi_arc` input is driven by CEC. The edit to `source.rs` is the offer decision alone (the
   `capture` track owns the file).
6. **CEC never stops the endpoint.** The adapter is opened and claimed on the CEC thread and
   retried every 10 s after a failure or after it goes away; the endpoint plays on. Only a flag
   the bus cannot carry (an OSD name that is not 1 to 14 printable ASCII bytes, a `--cec-*` flag
   without `--cec`) is a refused configuration.

## Deviations from the envelope

- **System Audio Mode off does not mute the room** (CTS 11.2.15-17 expects it): a chorus room
  plays other sources too, and the TV's standby already ends the TV input. On does unmute
  (11.2.15-16).
- **The standby reason on the wire** is not added here: the `theater` track adds a `reason` to
  `source_offer` (`signal_reason::STANDBY`) and `stop_on_standby` to the autoplay rule in
  parallel. This change logs `reason=standby` on the hub and offers `signal = false` at once;
  until the reason travels, the server stops a TV autoplay after its 30 s hold instead of at
  once. Setting the offer's `reason` from `SignalReason::Standby` is a one-line follow-up for
  whichever of the two merges second (or the integration track).
- **The TV is polled** with Give Device Power Status at start and every 30 s, beside the
  envelope's announcements, as the research recommends for TVs that announce nothing.

## Evidence

- `crates/cec/tests/audio_system.rs`: one test per CTS case (11.2.15-1..9, -13, -14, -16, -17
  as a named difference, -18; 11.2.13-1..4; 11.2.17-1..4; 12-2), the TV power detection
  table, the unprompted report, the power poll, configuration refusals, and on the fake bus the
  four scripted TVs (Roku-like, Feature-Abort-everything, no-ARC, slow), a second Audio System
  refused, a flood reported lost, a hot plug re-announced, the bus closing.
- `crates/cec/tests/fixtures.rs`: every vector round trips and validates as its fields say; the
  role produces every vector it sends byte for byte.
- `crates/server/tests/cec_tv.rs`: on the real `chorus-server`, the TV's System Audio Mode
  request is broadcast on; five Volume Up presses move the den from 0.500 toward 0.600 and stop
  at its 0.560 limit (a sixth changes nothing); Report Audio Status says 56, pushed and asked;
  the TV turning on raises the TV input's signal with no audio at all, the den's autoplay plays
  it; the standby ends the signal at once and the den is restored.

Nothing here is timing evidence (BRIEF.md section 3.1 rule 3): the bus is in memory.

## Not chosen

- **libcec, or the `cec_linux` crate as a dependency**: libcec is GPL (clean room); the crate
  is MIT but would add `nix` and a transposed `features` declaration for a binding of ten
  ioctls. Read and cited, not depended on.
- **Normal (non-passthrough) follower mode**, letting the kernel's core answer version, vendor
  ID, OSD name and physical address: fewer lines, but then the role cannot be tested as one
  thing, and the user-control messages still need a follower.
- **Answering volume keys locally** (a hub-side gain): it would bypass the server's limits and
  quiet hours (I10) and the room's other speakers.
- **Refusing to start when the adapter is missing**: a TV that is off gives a Pi no physical
  address; a speaker that will not play because a TV is off is the wrong failure.

## ASSUMED values (not measured)

- `--cec-arc` off; `--cec-osd-name` `chorus`; the Give Device Vendor ID abort reason
  (Unrecognized opcode).
- The volume step, 2 points per press and per repeat (`crate::cec::VOLUME_STEP`).
- The unprompted report window, 2 s after a TV volume key (`REPORT_AFTER_KEY_MS`).
- The TV power poll, every 30 s (`POWER_POLL_MS`).
- The reopen wait, 10 s; the CEC thread's turn, 20 ms; the error back-off, 500 ms.
- The fake bus's receive queue depth, 64.
- `/dev/cecN` group `video` (LEAD) and the Pi's `/dev/cec0`/`/dev/cec1` per connector (LEAD).
- Cited, not assumed: the 1 s response time (CTS 11.2.15-18), the 550 ms key release timeout
  (Android `FOLLOWER_SAFETY_TIMEOUT`), the SAD bytes (Android), the ioctl numbers and layouts.

## Follow-ups

- **The standby reason**: set `source_offer.reason = signal_reason::STANDBY` from
  `SignalReason::Standby` once the `theater` track's field is on main (above).
- **A TV key detaches a TV autoplay**: a `volume_step` from the hub is "a person's command" to
  the schedule runtime, which detaches the room from the autoplay, so the TV's standby no longer
  stops it (`cec_tv.rs` presses its keys before the rule exists for that reason). The
  integration track should let a TV input's own hub adjust volume without detaching, or not
  count volume commands as detaching.
- **Bench**: the owner's session on a real TV (`docs/cec.md`, "Bench steps"), including whether
  a Roku TV forwards volume keys to an Audio System that is not on its ARC input (LEAD,
  `cec.md` section 3); a `--cec-phys-addr` or EDID-follow for a Pulse-Eight dongle if the bench
  uses one; vivid as a host-CI tier.
- **The package**: a CEC drop-in shipped beside the front-panel one (today `docs/cec.md` gives
  the lines to add).
