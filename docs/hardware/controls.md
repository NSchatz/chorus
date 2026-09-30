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
| mic mute switch | latching | nothing; the microphone gate closes (below) |

A contact is believed after 20 ms at one level (ASSUMED debounce). Every volume a button asks
for is a request the server clamps by the room's limits (docs/protocol.md, K81, I10).

## The status LED

One state at a time, in this priority: fault, pairing, microphone muted, booting, link down,
playing, idle. Each has a fixed dim colour (ASSUMED palette in `firmware/src/controls.c`): fault
red, pairing blue, muted orange-red, booting white, link down amber, playing and idle white at
different brightness. While playing, a light that follows the visualizer shows the last `color`
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
