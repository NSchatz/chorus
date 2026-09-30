# 0063: a speaker's buttons are the controller role, its status LED follows the visualizer, and its microphone sits behind a gate that starts closed

- Status: accepted (goal 9, 2026-09-30)
- Decided by: the goal (brief section 13 item 2; K65, K67-K70, K81, K92, I4, I10)
- Implemented in: `firmware/src/controls.c`, `firmware/include/chorus/controls.h`,
  `crates/server/src/controller.rs`; held by `firmware/tests/test_controls.c`
  (`make -C firmware controls`, in `make firmware-check`), `crates/server/tests/controller_role.rs`
  and the shared fixtures `fixtures/controls/`; hardware notes in `docs/hardware/controls.md`

## Context

K65 gives protocol v2 a controller role (a speaker's buttons control its room or group) and a
visualizer role (a light stream for speaker LEDs); goal 5 built the messages
(`controller_command`, `controller_state`, `visualizer_frame`, `color`). K67-K70 give each class
its controls: the compact speaker buttons or touch, a status LED and a microphone with a hardware
mute switch; the two-way a hidden pairing button and a rear status light; the subwoofer a pairing
button, a status LED and level and phase knobs; the streaming amp a pairing button, an LED and
front buttons. Nothing on the endpoint read a control, and the server did nothing with a
`controller_command`.

## What was read

All read 2026-09-30: the brief's K65, K67-K70, K81, K92, I4, I10 and section 4.8;
`docs/protocol.md` (controller and visualizer sections, and why the controller role is not a
second control plane); `firmware/include/chorus/protocol_v2.h`, `firmware/src/protocol_v2.c`;
`crates/protocol/src/v2/{catalog,messages,codec}.rs`; `crates/control/src/{catalog,zones}.rs`;
`fixtures/protocol/v2/controller_command_*`, `visualizer_frame.*`, `color.*`;
`tools/conventions/check-shared-fixtures.sh`. No GPL source.

## Decision

1. **One pure C core per endpoint, a profile per class.** `controls.c` takes levels and ADC codes
   with the monotonic time they were sampled (never a clock read of its own, BRIEF.md guardrail
   4) and hands back actions. A class lists exactly its K67-K70 inputs; an input it does not have
   is refused and counted, never acted on.
2. **Buttons are the controller role.** A press becomes a protocol v2 `controller_command`
   encoded by the session's own encoder: play/pause toggles on release; held 1 s it leaves a group
   (when `controller_state` says the room plays in one) or joins the configured target; volume
   steps by 5 and repeats while held; next and previous. The server's `controller::translate`
   turns a command into the control catalog's own `Command` (volume, mute, group, ungroup),
   applied by `Zones::apply` with its checks, clamped into range, so a button is never a bypass
   (I10); volume points map linearly to thousandths (ASSUMED until goal 11's volume model).
   Transport commands (play, pause, toggle, next, previous) come back as a `TransportRequest` for
   the input path of goals 16 and 17; no input exists to act on yet.
3. **The pairing button has no protocol message.** Adoption is trust-on-first-use (K92) and the v2
   catalog has no pairing message; a press is a local event (the LED shows pairing) until goal 14
   decides what it asks for.
4. **The sub's knobs are local sound settings** (K69, beside the app's): level is a cut only,
   -12.0 to 0.0 dB, so no knob position exceeds the room's limit (K81, I10); phase 0 to 180
   degrees; both quantised with ADC hysteresis. They reach the DSP in goal 12; the v2 catalog has
   no message for them, so they are reported locally.
5. **The status LED** has one state by priority (fault, pairing, muted, boot, link down, playing,
   idle) and a fixed dim palette (ASSUMED). While playing, a class whose LED follows the
   visualizer shows the last `color` heard scaled by the last frame's peak, full brightness on a
   beat of 128 or more, and returns to the steady colour after 500 ms without a frame. Frames are
   held until their server-timeline timestamp is heard. The two-way's rear light never follows
   (K68's clean front). Fades (`transition_ms`) are not rendered yet.
6. **The microphone gate starts closed** and opens only on a debounced reading of the switch in
   the live position; `chorus_controls_mic_pass` is the only path for microphone samples. The
   switch also breaks the microphone's supply in hardware (`docs/hardware/controls.md`).
7. **Shared fixtures.** `fixtures/controls/<class>.hex` are the frames a scripted run of each
   class produces; the C test must produce them byte for byte and the Rust test decodes and
   applies them to a room. `visualizer-sequence.{hex,led}` is the LED's fixture until goal 12
   computes the stream.

## ASSUMED values (not measured)

Debounce 20 ms, long press 1 s, repeat after 600 ms then every 300 ms, volume step 5, knob
hysteresis 24 codes, visualizer stale after 500 ms, beat threshold 128, the palette, and the
linear volume mapping. The binding to GPIO, touch, LEDC and ADC is not written: the reference
board has none of these controls and the owner's boards are unidentified (Needs item "Your
ESP32-S3 boards: module markings and a read-only chip report"); goals 24-26 place them.

## Consequences

- A button press and the server's resulting room state are checked against one committed file.
- Transport commands wait on inputs (goals 16, 17); pairing on adoption (goal 14); knob values on
  the DSP (goal 12); a real visualizer stream on goal 12.
