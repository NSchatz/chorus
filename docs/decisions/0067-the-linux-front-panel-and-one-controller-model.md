# 0067: one controller model in two languages, a Linux front panel read through evdev and the LED class, and the server routes an endpoint's controller commands through the control plane

- Status: accepted (goal 10, 2026-09-30)
- Decided by: the goal (brief section 14 item 3; K65, K70, K74, K81, K96, I10), within ADR 0063
- Implemented in: `crates/controls` (the Rust twin of `firmware/src/controls.c`),
  `crates/client-linux/src/front_panel.rs` (the Linux binding), `session.rs`, `config.rs` and
  `main.rs` in `crates/client-linux` (`--front-panel`, the roles in `hello`),
  `crates/server/src/session.rs` (`route_controller`), `crates/server/src/control.rs`
  (`ControlState::controller`), `config/front-panel/rack-amp.conf` (the ASSUMED example); held
  by `crates/controls/tests/shared_fixtures.rs`, `crates/server/tests/front_panel.rs` and the
  unit tests in `front_panel.rs`

## Context

Goal 9 (ADR 0063) built the controls of every speaker class as a pure C core,
`firmware/src/controls.c`, held to the committed `fixtures/controls/*`, and a server-side
`controller::translate`; the server's runtime did not route an endpoint's `controller_command`
anywhere (goal 9's adversarial check said so). Goal 10 makes Linux endpoints a product tier
(K96), first the 2U rack amp (K74), which is K70's streaming amp in a rack case: front buttons, a
pairing button and a status LED. Brief section 14 item 3 asks for its buttons and LED through
Linux GPIO as the controller role, with one controller model shared with the firmware.

## What was read

All read 2026-09-30.

- `https://docs.kernel.org/input/input.html` (the event interface: `struct input_event`, whole
  events per read, `EV_KEY` values 0 release, 1 press, 2 autorepeat) and
  `https://docs.kernel.org/input/event-codes.html` (`EV_SYN`, `SYN_REPORT`; no numeric values).
- `https://docs.kernel.org/leds/leds-class.html` (`/sys/class/leds/`, `brightness`,
  `max_brightness`) and `https://docs.kernel.org/leds/leds-class-multicolor.html`
  (`multi_index`, `multi_intensity`, "led_brightness = brightness * multi_intensity/max_brightness").
- `https://www.kernel.org/doc/Documentation/devicetree/bindings/input/gpio-keys.yaml`
  (`debounce-interval`, "If not specified defaults to 5" ms; `linux,code`; `autorepeat`).
- The Raspberry Pi overlays README,
  `https://github.com/raspberrypi/firmware/blob/master/boot/overlays/README` (`gpio-key`: gpio,
  active_low, gpio_pull, label, keycode, autorepeat; `gpio-led`: gpio, label, trigger, and
  "echo 1 > /sys/class/leds/myled1/brightness"; `pwm`).
- Permissive source: the `evdev` crate 0.13.2 (Apache-2.0 OR MIT),
  `https://docs.rs/evdev/0.13.2/src/evdev/scancodes.rs.html` (`KEY_VOLUMEDOWN = 114`,
  `KEY_VOLUMEUP = 115`, `KEY_NEXTSONG = 163`, `KEY_PLAYPAUSE = 164`, `KEY_PREVIOUSSONG = 165`,
  `KEY_CONNECT = 218`) and `.../constants.rs.html` (`SYNCHRONIZATION = 0x00`, `KEY = 0x01`,
  `SYN_REPORT = 0`); the `libc` crate 0.2.189 (MIT OR Apache-2.0),
  `https://docs.rs/libc/0.2.189/src/libc/unix/linux_like/linux/mod.rs.html` (`input_event` is a
  `timeval` then type, code, value on 64-bit targets).
- In this repository: `firmware/include/chorus/controls.h`, `firmware/src/controls.c`,
  `firmware/tests/test_controls.c`, `fixtures/controls/*`, ADR 0063 and 0065,
  `docs/protocol.md` ("The four roles", "Controller"), `crates/protocol/src/v2`,
  `crates/server/src/{controller,control,session,clients}.rs`, `crates/client-linux/src/*`.
- Not opened: the Linux kernel's source and headers (GPL), including
  `include/uapi/linux/input-event-codes.h` which the Pi README links to; libgpiod.

## Decision

1. **One model, a Rust twin held to the C model's fixtures.** `crates/controls` (a new pure
   crate, `#![forbid(unsafe_code)]`, depending only on `chorus-protocol`) is `controls.c` line
   for line: the four classes, the nine inputs, the 20 ms debounce, the 1 s long press, the
   600/300 ms volume repeat, the knob quantisation and hysteresis, the microphone gate, the
   actions, the LED's state priority, palette and visualizer following, every ASSUMED constant
   the same. `crates/controls/tests/shared_fixtures.rs` drives the exact scripts of
   `test_controls.c` and must produce `compact.hex` and `streaming-amp.hex` byte for byte and
   every moment of `visualizer-sequence.led`; the fixtures are not changed, and
   `check-shared-fixtures.sh` now names this test as a Rust reader of `fixtures/controls`. A
   crate rather than a module of `chorus-protocol` because the protocol crate is the catalog and
   the codec, and the model is behaviour both endpoints share, like `chorus-sync` (CLAUDE.md
   rule 5: protocol, sync and DSP cores as pure libraries with shared fixtures).
2. **The rack amp is the streaming-amp class**, not a new class: K74's rack amp is K70's
   streaming amp in a 2U case, with the same controls. No behaviour was added, so no fixture
   was. A multi-zone front panel (choosing which zone the buttons control) waits on P13's zone
   count (goal 26); until then the panel controls the one room it is configured for.
3. **Buttons through gpio-keys and evdev, the LED through the LED class**, not the GPIO
   character device. Reasons, on fitness alone: reading `/dev/input/eventN` is a plain `read` of
   fixed 24-byte records (no ioctl, so no `unsafe` and no new place on conventions rule 2's list,
   where the character device's line requests are ioctls); the kernel debounces in gpio-keys
   (5 ms default) before the shared model's own 20 ms, so a Linux panel and an ESP32 panel
   behave alike; the device-tree overlays are stock on a Raspberry Pi (`gpio-key`, `gpio-led`)
   and the lines are configured by the boot firmware before the service starts; and an
   unprivileged service needs only read access to one input device and write access to one LED
   directory, which a group or udev rule grants (ASSUMED deployment detail, the package track's),
   where the character device hands a process whole lines. A 32-bit target's `input_event`
   differs; chorus packages arm64 and x86_64 only, so the record is 24 bytes, native order.
4. **Every event is stamped with the monotonic timeline when its read returned**, never with
   the record's own `time` (a `timeval` the kernel fills from a clock this process does not
   choose; BRIEF.md guardrail 4). The model dates each action from those stamps. Autorepeat
   (value 2) is ignored: the model repeats by its own rule. `front_panel.rs` and the controls
   crate are listed in `audio-path.conf` so the wall-clock check covers them.
5. **A configuration maps key codes and the LED.** `--front-panel <file>`: `class`, `room`,
   `join-target`, one or more `input` devices, `key <code> <control>` lines and `led <dir>`. A key
   mapped to a control the class does not have is refused at load. `config/front-panel/rack-amp.conf`
   is the example, ASSUMED throughout (the board and pins are unanswered owner inputs), and a
   unit test parses it. A single-colour LED (`gpio-led`, `max_brightness` 1) shows the model's
   brightness; a multicolour one also gets `multi_intensity` in its `multi_index` order.
6. **The controller role in the client.** With a panel, `hello` declares `controller` (and
   `visualizer` when the class's light follows it and a light is configured, with `led_count` 1).
   The panel's commands go out on the session's writer, shared with the time-sync exchange under
   one lock per whole frame; `controller_state`, `visualizer_frame` and `color` from the server
   reach the panel through the session's message handler. A command decided while no session is
   up is counted unsent, never queued for later.
7. **The server routes controller commands through the control plane.** The session reader of
   an endpoint that declared the controller role hands each `controller_command` to
   `ControlState::controller`: the zone is the one the endpoint (its authenticated session id)
   is attached to, `controller::translate` turns the command into the catalog's own `Command`,
   and `Zones::apply`, persistence and the fanout run exactly as for `POST /api/command`, so a
   button is never a bypass (K81, I10; goal 11's room limits apply there). The endpoint is
   answered with a `controller_state` (volume in points, mute, `playing`, its group) on its own
   outbound queue, never blocking the reader. A command from a peer that did not declare the
   role, from an endpoint attached to no zone, or to a server without a control plane changes
   nothing and is logged by name. Transport commands are logged as waiting for an input (goals
   16, 17), as ADR 0063 decided.

## Not chosen

- The GPIO character device uAPI v2: fitter for toggling an output at a planned instant, but
  for buttons it needs ioctls, an `unsafe` place and debounce in user space; for the LED the LED
  class is the kernel's own abstraction and accepts `gpio-led`, PWM and multicolour drivers alike.
- A new `rack-amp` class: nothing K70 and K74 ask of the front panel differs from the streaming amp.
- Using the event's kernel timestamp: not the process's monotonic timeline.
- The endpoint's self-declared `hello` name to find its zone: the session's authenticated id is
  the one the server pinned, so it is the one the zone lookup trusts. The client's default id is
  its `--endpoint` name, which is what it attaches as.

## ASSUMED values

Every model constant of ADR 0063 (unchanged); the 5 ms model poll interval; every path, pin and
key code of `config/front-panel/rack-amp.conf`; that one input device serves all keys (else one
`input` line each); the LED is shown against the local monotonic timeline, not the server's (the
sync offset is not yet published to it; harmless until a server sends a visualizer stream, goal
12); `controller_state.playback` is `playing` while the server streams.

## The GPIO marker (goal 9's follow-up, ADR 0065)

Not built here. ADR 0065's marker toggles a pin when a server-timeline boundary frame reaches
the DAC, planned from the device delay and fired from a hardware timer, so a logic analyzer
reads the edge delta between two endpoints. On Linux the same needs (a) a pin, and the Linux
board is an unanswered owner input; (b) a toggle planned inside the playout loop
(`crates/client-linux/src/run.rs`, the audio path, which the multichannel track is changing in
parallel) from ALSA's reported delay, and fired by a timed user-space wait whose latency is
unmeasured on any Linux board here; and (c) an output that can be set at an instant: the LED
class's `brightness` write goes through the driver's own path with no timing contract, so it
does not meet the rig's need, and the character device's line set is an ioctl, a new `unsafe`
place that needs its own ADR. A marker whose edge error is unknown would make the cross-check
report a number it cannot stand behind. It belongs to goal 26, where the rack amp's Linux board
and its pins are designed, with a bench packet that measures the toggle latency (or earlier, if
the owner names the Linux board).

## Consequences

- A press on a Linux panel and on an ESP32 panel produce the same bytes, held by one set of files.
- An endpoint's buttons change its room on a running server; `crates/server/tests/front_panel.rs`
  runs it end to end on fakes (a pipe of `input_event` records, a fake LED class directory, the
  client's real session, the real `chorus-server` binary over loopback).
- The server now sends `controller_state` after each command; sending it on every state change
  (a change made by the app) is goal 11's, with rooms and groups.
