# HDMI-CEC on the hub: the TV's Audio System

The Linux hub (a `chorus-client` with the TV's sound on `--line-in`, kind `optical` or `hdmi_arc`)
can also be the TV's **Audio System** over HDMI-CEC: logical address 5, the address a soundbar
or AV receiver takes. Then the TV's own remote turns the room's volume up and down and mutes
it, the TV shows the room's level on screen, and the TV turning on or off starts or stops the
TV in the room. Goal 13 (the TV path); the decision record is ADR 0087
(`docs/decisions/0087-cec-audio-system-on-the-hub.md`).

Nothing here is timing evidence. The code is tested on a fake CEC bus with scripted TVs; the
bench steps below are the owner's, and a TV's real behaviour is a Needs item ("The three TVs:
model, eARC port, optical out and audio menu").

## How it is built

- `crates/cec`: the message codec, the Audio System role (a pure state machine), the kernel
  adapter over `/dev/cecN`, and a fake bus with four scripted TVs. Golden vectors in
  `fixtures/cec/` (Rust-only by declaration: CEC runs on the hub alone).
- `crates/client-linux/src/cec.rs`: the hub's CEC thread. TV keys become `controller_command`s
  on the hub's session (the controller role, as a front panel's buttons are); the server's
  `controller_state` for the hub's room becomes the CEC answers; the TV's power goes to the
  source role.
- The kernel's CEC API, never libcec: `/dev/cecN` opened read-write, taken as initiator and
  exclusive passthrough follower (`CEC_MODE_INITIATOR | CEC_MODE_EXCL_FOLLOWER_PASSTHRU`, so
  the hub answers every message itself, [K-MODE]), one logical address of type Audio System
  claimed with `CEC_ADAP_S_LOG_ADDRS` ([K-LOG]), then `CEC_TRANSMIT`, `CEC_RECEIVE` and
  `CEC_DQEVENT` ([K-RX], [K-EV]). The struct layouts and ioctl numbers come from the kernel's
  documentation prose and the MIT `cec_linux` crate, recomputed and size-asserted at compile
  time (`crates/cec/src/kernel.rs` cites each).

Clean room (BRIEF.md guardrail 1, K33): no GPL source was opened (not libcec, not v4l-utils'
`cec-ctl`, `cec-follower` or `cec-compliance`, not the kernel's C source or uapi headers, not
vivid). The behaviour is modelled on Android's Apache-2.0 HDMI service and its CTS tests.

## What the hub answers

Directly addressed to 5 unless said. "CTS" is the HDMI CTS case as Android's CTS host tests for
an audio device name it ([CTS], Apache-2.0).

| The TV (or any device) sends | The hub | Source |
|---|---|---|
| System Audio Mode Request [path] | broadcasts Set System Audio Mode [on]; unmutes a muted room | CTS 11.2.15-1, -16 |
| System Audio Mode Request (no operand) | broadcasts Set System Audio Mode [off]; does NOT mute the room (below) | CTS 11.2.15-5 |
| Give System Audio Mode Status | System Audio Mode Status | CTS 11.2.15-4, -7 |
| Give Audio Status | Report Audio Status: the room's volume 0 to 100, mute in bit 7 | CTS 11.2.15-9 |
| User Control Pressed Volume Up / Down | `controller_command` `volume_step` +2 / -2 points, each press and each repeat | CTS 11.2.13-1..4 |
| User Control Pressed Mute | `mute_set` to the opposite of the room's mute, once per press | CTS 11.2.15-8 |
| User Control Pressed Mute Function / Restore Volume Function | `mute_set` 1 / 0 | |
| User Control Pressed Power keys | accepted, nothing done (the hub has no standby) | |
| Give Device Power Status | Report Power Status [on] | |
| Give OSD Name | Set OSD Name (`--cec-osd-name`) | |
| Give Physical Address | broadcast Report Physical Address [address][5] | |
| Get CEC Version | CEC Version [1.4] | |
| Give Device Vendor ID | Feature Abort [Unrecognized opcode]: chorus has no IEEE OUI (the reason is ASSUMED) | |
| Request Short Audio Descriptor | LPCM, 2 channels, 48 kHz, 16 bit only (`09 04 01`); a request naming no LPCM is Feature Aborted [Invalid operand]. Never AC-3, E-AC-3, DTS or anything else (P2 Option A) | CTS 11.2.15-13, -14 |
| Request ARC Initiation / Termination, Report ARC Initiated / Terminated | with `--cec-arc`: Initiate ARC / Terminate ARC; without: Feature Abort [Unrecognized opcode] | CTS 11.2.17-1..4 |
| Abort | Feature Abort [Refused] | [K-MODE] |
| Standby (from the TV, or broadcast) | System Audio Mode off is broadcast first if it was on; the TV is in standby | CTS 11.2.15-6 |
| anything else, directly addressed | Feature Abort [Unrecognized opcode] | Android |

Never answered: broadcasts, anything from Unregistered (15), a Feature Abort, and a message
that is malformed for its opcode in the ways Android drops (wrong initiator, a direct-only
message received as broadcast or the reverse, too few operands; CTS 12-2). An operand out of
range in a directly addressed message is answered Feature Abort [Invalid operand]. The rules
are Android's `HdmiCecMessageValidator` (`crates/cec/src/validate.rs`).

The server decides what a key changes and clamps it by the room's limit and quiet hours, as it
does for every volume path (K81, I10); the hub only asks. After a change a TV key caused (within
2 s, ASSUMED) and with System Audio Mode on, the hub sends the new Report Audio Status to the TV
unprompted, so its on-screen level follows.

### Deliberate differences from an AV receiver

- **System Audio Mode off does not mute the room** (CTS 11.2.15-17 expects a mute). A chorus
  room is shared with every other source: a TV turning its own speakers back on must not silence
  the music playing there. The TV input's signal ends instead (standby, below).
- **The hub never goes to standby**: power keys do nothing and Give Device Power Status is always
  "on". The hub is a server endpoint; its speakers are the room's.
- **A Short Audio Descriptor request is answered in any System Audio Mode** (Android only with it
  on). The answer is the same either way.
- **The hub does not ask for System Audio Mode on its own** by default (CTS 11.2.15-2 describes a
  device that does; the role can, `initiate_system_audio`, and is tested doing it): the TV asks
  when its own "System audio control" setting is on (on a Roku TV: Settings > System > Control
  other devices (CEC), [ROKU-HDMI]).

## The TV's power and the TV input

The hub learns the TV is **on** from: Report Power Status [on] or [standby to on] from the TV;
Active Source, Routing Change or Set Stream Path from anyone; Image View On or Text View On sent
to the TV (seen only where the adapter shows other devices' messages); a System Audio Mode
Request with a path, or a Request ARC Initiation, from the TV. It learns the TV is in **standby**
from: Standby from the TV or broadcast by anyone; Report Power Status [standby] or [on to
standby]. It also asks the TV with Give Device Power Status at start and every 30 s (ASSUMED),
the fallback `cec.md` recommends for TVs that announce nothing.

The TV input's offered `signal` (the `source_offer` the server's autoplay reads) is:

> audio present OR (the TV is on AND `--cec-autoplay-on-power`)

and a standby ends it **at once**, without the 2 s silence hold, and keeps it ended until the
audio detector has let the held audio go and found audio again, or the TV is seen on again
(`chorus_cec::TvSignal`). The offer's status line names why: `reason=audio`, `tv-on`, `quiet`
or `standby`. CEC drives a TV input only: an `optical` or `hdmi_arc` line-in, never an analogue
`line_in`.

The room's autoplay rule for the TV input (K81) then plays the TV when it turns on and stops it
when the signal goes, restoring what the room played. Today the server stops it after the
autoplay hold (30 s); the `theater` track's `stop_on_standby` stops it at once when the offer
carries the standby reason (see the ADR's follow-ups).

## Flags

| Flag | Default | What |
|---|---|---|
| `--cec <device>` | none | Be the TV's Audio System on this adapter (`/dev/cec0`). Declares the controller role. |
| `--cec-arc` | off (ASSUMED) | The hub's HDMI port is the TV's ARC port and ARC is wanted. P2's ARC path goes through an extractor's optical output, and a Raspberry Pi's HDMI port is a source that cannot receive ARC audio (`cec.md` section 3, LEAD). |
| `--cec-autoplay-on-power on\|off` | on | The TV turning on offers the TV input before any audio arrives. |
| `--cec-osd-name <name>` | `chorus` (ASSUMED) | The name the TV shows: 1 to 14 printable ASCII bytes, refused otherwise. |

Any `--cec-*` flag without `--cec` is a refused configuration (exit 2). The adapter is opened
and address 5 claimed on the CEC thread: one that cannot be opened or claimed (no such device,
no permission, another Audio System on the bus, no physical address because the TV is off or
its CEC is disabled) is logged as `cec unusable detail="..." retry_s=10` and tried again every
10 s (ASSUMED); the endpoint plays on, because CEC is the TV's remote, never a reason not to play.
Other status lines: `cec started`, `cec adapter`, `cec claimed logical_address=5
physical_address=a.b.c.d`, `cec tv-power state=on|standby`, `cec system-audio-mode on=1|0`,
`cec command=volume_step value=2 key=volume-up sent=1`, `cec transmit status=nack ...` (a
message nobody acknowledged), `cec stopped reason=adapter-gone`.

The volume step (2 points per press and per repeat, ASSUMED: a TV repeats a held key several
times a second) is smaller than a front panel's 5.

## Bench steps (the owner's)

None of this is run by the repository. With `chorus-client` stopped:

### A Raspberry Pi's own HDMI port

1. With the KMS display driver (`dtoverlay=vc4-kms-v3d`, the default in current Raspberry Pi
   OS), each HDMI connector has its own CEC adapter, `/dev/cec0` and `/dev/cec1` (forum-sourced,
   LEAD: Raspberry Pi forums t=316138, t=327247). Which node is which connector:
   `ls -l /sys/class/cec/` (`cec.md` section 5).
2. Plug the Pi's HDMI into a TV input (any; the ARC input only if ARC is wanted), turn the TV
   on and its CEC on (Roku TV: Settings > System > Control other devices (CEC) [ROKU-HDMI]).
3. Smoke test with the kernel's tools (binaries from `v4l-utils`; using them is fine, their
   source is not opened): `cec-ctl -d /dev/cec0 --playback` then `cec-ctl -d /dev/cec0 -S`
   shows the topology and the Pi's physical address [K-CEC]. Clear it again before chorus runs:
   chorus takes the adapter exclusively.
4. Give the service the device: `sudo systemctl edit chorus-client` and add

   ```
   [Service]
   SupplementaryGroups=video
   DeviceAllow=/dev/cec0 rw
   ```

   (`/dev/cecN` is group `video` on most distributions: LEAD, check `ls -l /dev/cec0`; the
   unit's `DevicePolicy=closed` needs the `DeviceAllow=` line, systemd.exec(5)).
5. Add `--cec /dev/cec0` to `CHORUS_CLIENT_ARGS`, restart, and read `journalctl -u
   chorus-client` for `cec claimed logical_address=5`.
6. On the TV, turn on system audio control; press volume up. The journal shows `cec
   command=volume_step value=2 ... sent=1` and the control page's room volume moves; the TV's
   on-screen level should follow. `cec-ctl -d /dev/cec1 -M` on a second adapter on the same bus
   monitors the traffic [K-CEC].

### A Pulse-Eight USB-CEC adapter

It enumerates as `/dev/ttyACM*` and becomes `/dev/cecN` through the kernel's `pulse8-cec`
driver with `inputattach --pulse8-cec /dev/ttyACM0` (package `inputattach`), which the kernel's
admin guide shows run from a udev rule and a systemd unit [K-CEC]:

```
SUBSYSTEM=="tty", KERNEL=="ttyACM[0-9]*", ATTRS{idVendor}=="2548", ATTRS{idProduct}=="1002", ACTION=="add", TAG+="systemd", ENV{SYSTEMD_WANTS}+="pulse8-cec-inputattach@%k.service"
```

```
[Unit]
Description=inputattach for pulse8-cec device on %I

[Service]
Type=simple
ExecStart=/usr/bin/inputattach --pulse8-cec /dev/%I
```

The dongle has no EDID of its own: its physical address must be set (`CEC_CAP_PHYS_ADDR`,
[K-PHYS]), for example from a DRM connector's EDID with `cec-ctl -E
/sys/class/drm/card0-HDMI-A-1/edid` [K-CEC]. chorus does not set it (a follow-up if a bench
needs it). Then steps 4 to 6 above.

### Without a TV

`modprobe vivid` (root, on a host, not in a container) gives a pair of emulated CEC adapters
wired together [K-VIVID]; `cec-follower` emulates a TV on one and chorus runs on the other. A
bench tier, not a gate step.

## Sources (read 2026-10-01)

- [K-MODE] https://docs.kernel.org/userspace-api/media/cec/cec-ioc-g-mode.html
- [K-LOG] https://docs.kernel.org/userspace-api/media/cec/cec-ioc-adap-g-log-addrs.html
- [K-PHYS] https://docs.kernel.org/userspace-api/media/cec/cec-ioc-adap-g-phys-addr.html
- [K-RX] https://docs.kernel.org/userspace-api/media/cec/cec-ioc-receive.html
- [K-EV] https://docs.kernel.org/userspace-api/media/cec/cec-ioc-dqevent.html
- [K-CEC] https://docs.kernel.org/admin-guide/media/cec.html
- [K-VIVID] https://docs.kernel.org/admin-guide/media/vivid.html
- [CTS] https://android.googlesource.com/platform/cts/+/refs/heads/main/hostsidetests/hdmicec/src/android/hdmicec/cts/audio/
  (Apache-2.0)
- Android's HDMI service, Apache-2.0:
  https://android.googlesource.com/platform/frameworks/base/+/refs/heads/main/services/core/java/com/android/server/hdmi/
  (`HdmiCecMessageValidator`, `HdmiCecLocalDeviceAudioSystem`, `HdmiCecLocalDevice`,
  `HdmiCecController`, `HdmiCecMessageBuilder`, `Constants`)
- The `cec_linux` crate 0.2.2, MIT: https://github.com/User65k/cec_linux
- [ROKU-HDMI] https://support.roku.com/en-ca/article/configure-hdmi-settings-on-your-tv
- Goal 13's research file `cec.md` (the coordinator's, with the full source list).
