# Controls, the status LED and the microphone mute, per speaker class

What each class carries (K67-K70), how the firmware reads it
(`firmware/include/chorus/controls.h`, `docs/decisions/0063-*`), and the hardware rule for
the microphone mute switch. Pins are not given: the owner's boards are not identified yet
(the Needs item "Your ESP32-S3 boards: module markings and a read-only chip report"), and the
reference board profile (`firmware/boards/brick-s3-wired.conf`, ASSUMED) has no buttons, LED,
knobs or microphone. A board profile that adds them names its pins; until then every control
pin is absent and the image drives none. The speaker designs (goals 24-26) place them.

## Per class

| Class | Controls (K67-K70) | Light | Microphone |
|---|---|---|---|
| compact | buttons or touch: play/pause, volume up, volume down, next, previous | status LED, follows the visualizer while playing | yes, behind a hardware mute switch |
| two-way | a hidden pairing button | a rear status light, status only (never the visualizer, so the front stays clean) | no |
| subwoofer | a pairing button; level and phase knobs | status LED, follows the visualizer while playing | no |
| streaming amp | a pairing button; front buttons: play/pause, volume up, volume down, next, previous | status LED, follows the visualizer while playing | no |

The streaming amp's line-in, optical in and line/sub out (K70) are inputs and outputs, the
source role of goal 17, not controls.

## What a control does

| Control | Gesture | What leaves the endpoint |
|---|---|---|
| play/pause | short press (toggles on release) | `controller_command` toggle |
| play/pause | held 1 s (ASSUMED) | `leave` when the room plays in a group, else `join` the configured target; with no target configured nothing is sent and the console says why |
| volume up / down | press, then repeats every 300 ms after 600 ms held (ASSUMED) | `volume_step` +5 / -5 (ASSUMED step) |
| next / previous | press | `next` / `previous` |
| pairing | press | nothing yet: a local event (adoption is trust-on-first-use, K92, and goal 14 decides what a press asks for); the LED shows pairing |
| sub level knob | turn | nothing: a local setting, -12.0 dB to 0.0 dB in 0.5 dB steps, a cut never a boost (K81, I10) |
| sub phase knob | turn | nothing: a local setting, 0 to 180 degrees in 15 degree steps |
| mic mute switch | latching | a `mic_state` (the gate, muted or live) on a voice endpoint's session; the microphone gate closes or opens (below) |

A contact is believed after 20 ms at one level (ASSUMED debounce). Every volume a button asks
for is a request the server clamps by the room's limits (docs/protocol.md, K81, I10).

## The status LED

One state at a time, in this priority: fault, pairing, microphone muted, booting, link down,
listening, playing, idle. Each has a fixed dim colour (ASSUMED palette in
`firmware/src/controls.c`): fault red, pairing blue, muted orange-red, booting white, link down
amber, listening green-cyan, playing and idle white at different brightness. While playing, a light that follows the visualizer shows the last `color`
message heard, scaled by the last `visualizer_frame`'s peak, with a beat of 128 or more at the
colour's full brightness; with no frame for 500 ms it returns to the steady playing colour.
Frames carry server-timeline timestamps and are shown when heard. A `color` message's
`transition_ms` fade is not rendered yet: the colour changes when it is heard.

## The microphone mute switch (compact class)

- **Hardware:** the switch physically breaks the microphone's supply (or its data line, for a
  digital microphone whose supply cannot be switched) so no firmware fault can un-mute it. A
  second pole of the same switch goes to a GPIO so the firmware knows the position. The switch
  is latching, not momentary. This is the rule for the compact design in goal 24; nothing is
  built here.
- **Firmware:** the only way microphone samples reach the voice path is
  `chorus_controls_mic_pass`, which passes nothing while the switch reads muted, and nothing
  before the switch has been read at all (the gate starts closed). The LED shows mute over
  every state but a fault or pairing. A speaker microphone is never a shareable source (I4); it
  feeds only the voice path (goal 20).

## The uplink, the switch and the listening light (compact class)

The session's voice path (`firmware/include/chorus/session.h`, `chorus_session_voice_*`;
docs/protocol.md, "The voice role") is where the switch meets the server's request. Host-tested
against a fake capture source (`firmware/tests/test_controls.c`, `make -C firmware controls`); no
microphone part or driver is chosen, so on a board the capture seam is empty and no audio is sent.

- **The uplink is the server's request; the switch is the endpoint's answer.** The server asks
  for microphone audio with `voice_control.uplink` (voice is enabled for the room). The endpoint
  sends `mic_audio` only while that request stands AND the gate is live. Neither is enough alone:

  | Switch (gate) | `uplink` | What leaves the speaker |
  |---|---|---|
  | muted, or not read yet | 0 or 1 | no `mic_audio`, for any input |
  | live | 0 | no `mic_audio` |
  | live | 1 | `mic_audio`: the captured samples, 16 kHz mono, stamped with the capture instant on the server timeline |

- **The switch always wins.** No message opens the gate: `uplink` is a request the gate can
  refuse, never an unmute. The samples the encoder is handed are the output buffer of
  `chorus_controls_mic_pass` and nothing else, so with the switch at mute there is nothing to
  encode. On the designed hardware the same switch has already cut the microphone's supply.
- **The switch is reported.** A voice endpoint tells the server its gate with a `mic_state` once
  after `capabilities` in every session and at every debounced change of the switch. A `muted`
  goes after the last `mic_audio`; a `live` goes before the first, and no sample leaves until it
  has been sent. The report tells the server what the switch says. It does not ask for anything.
- **A new session starts with the uplink off.** The request is the session's: after a reconnect
  nothing is sent until that session's server asks again. With no sync offset yet nothing is
  sent either (a capture instant is never guessed).
- **The listening light is the server's word, and only a light.** `voice_control.listening` says
  a voice run is open in the room; the LED then shows listening (above playing and idle, below
  everything else). It may be lit while this speaker sends nothing (`uplink` 0: another speaker
  of the room carries the run), and it changes nothing about what is sent. A speaker whose
  switch is at mute shows muted, not listening: the light a person reads for "this microphone
  is cut" is never replaced by the room's state. When the session ends the light goes out with it.
- **Classes without a microphone** (two-way, subwoofer, streaming amp) do not declare the voice
  role, never send a `mic_state` or a `mic_audio`, and never show listening.

## The Linux front panel (the rack amp and other Linux endpoints)

A Linux endpoint (K96; first the 2U rack amp, K74, which is the streaming-amp class in a rack
case) runs the same controller model as the firmware: `crates/controls` is the Rust twin of
`firmware/src/controls.c`, held to the same `fixtures/controls/*` (`docs/decisions/0067-*`). The
Linux binding is `crates/client-linux/src/front_panel.rs`, started by
`chorus-client --front-panel <file>`.

- **Buttons:** each GPIO line becomes a key of an input device through the kernel's gpio-keys
  driver, set up by a device-tree overlay (on a Raspberry Pi, one `dtoverlay=gpio-key,...` line per
  button: `gpio`, `active_low`, `gpio_pull`, `label`, `keycode`; overlays README,
  https://github.com/raspberrypi/firmware/blob/master/boot/overlays/README, read 2026-09-30).
  The client reads `/dev/input/...` as whole 24-byte `input_event` records (arm64 and x86_64),
  takes `EV_KEY` presses (1) and releases (0), ignores the kernel's autorepeat (2), and stamps
  each with the monotonic clock when the read returned, never the record's own time
  (https://docs.kernel.org/input/input.html, read 2026-09-30). gpio-keys debounces for 5 ms by
  default (its binding, read 2026-09-30); the model's own 20 ms (ASSUMED) runs on top.
- **The status LED:** the kernel's LED class, `/sys/class/leds/<name>/`. A single-colour LED
  (`dtoverlay=gpio-led,gpio=<n>,label=<name>`, `max_brightness` 1) is on while the model's
  brightness is not zero; a multicolour LED gets `multi_intensity` in its `multi_index` order and
  `brightness` scaled to `max_brightness`, which the kernel combines as "brightness *
  multi_intensity/max_brightness" (https://docs.kernel.org/leds/leds-class-multicolor.html, read
  2026-09-30). An RGB status light needs a board overlay for a multicolour LED (none of the Pi's
  stock overlays makes one; ASSUMED until the rack amp's board is designed, goal 26).
- **Permissions (ASSUMED, the package's to set):** the service reads one input device (group
  `input` or a udev rule) and writes one LED directory (a udev rule on its `brightness` and
  `multi_intensity`); it needs no other privilege.
- **The configuration:** `class`, `room` (the zone the buttons control; default `--zone`),
  `join-target`, one or more `input <device>` lines, `key <code> <control>` lines and
  `led <dir>`. A key mapped to a control the class does not have is refused at start.
  `config/front-panel/rack-amp.conf` is the ASSUMED example with the overlay lines that would
  produce it; the board, pins and paths are unanswered owner inputs.
- **What leaves the endpoint:** the model's `controller_command` frames, on the endpoint's v2
  session (its `hello` declares `controller`, and `visualizer` when a light follows it). The
  server applies each through its control plane to the zone the endpoint is attached to, with
  the same checks as any other change, and answers with `controller_state`; the panel uses it to
  decide whether a long press joins or leaves. Pairing stays a local event (the light shows it).
