//! The controls: classes, inputs, debounce, long press, repeat, knobs and the
//! microphone gate. Line for line the model of `firmware/src/controls.c`.

use chorus_protocol::v2::{self, ControllerCommand, ControllerState, Message};

/// ASSUMED: a contact is believed once it has held one level this long
/// (`CHORUS_CONTROLS_DEBOUNCE_NS`).
pub const DEBOUNCE_NS: u64 = 20_000_000;
/// ASSUMED: a press held this long is a long press.
pub const LONG_PRESS_NS: u64 = 1_000_000_000;
/// ASSUMED: a held volume button repeats after this ...
pub const REPEAT_DELAY_NS: u64 = 600_000_000;
/// ... then every this long.
pub const REPEAT_PERIOD_NS: u64 = 300_000_000;
/// ASSUMED: one volume press asks for this many points of 100.
pub const VOLUME_STEP: i16 = 5;
/// The knobs' ADC span (12 bits, the ESP32-S3's SAR ADC width).
pub const KNOB_MAX: u32 = 4095;
/// The subwoofer's level knob is a cut, never a boost: -12.0 dB to 0.0 dB in
/// tenths (K81, I10).
pub const SUB_LEVEL_MIN_TENTHS_DB: i32 = -120;
/// The level knob's step, in tenths of a dB.
pub const SUB_LEVEL_STEP_TENTHS_DB: i32 = 5;
/// The phase knob's range, 0 to this many degrees.
pub const SUB_PHASE_MAX_DEG: i32 = 180;
/// The phase knob's step.
pub const SUB_PHASE_STEP_DEG: i32 = 15;
/// ASSUMED: ADC codes of hysteresis before a knob moves to a neighbouring step.
pub const KNOB_HYSTERESIS: u32 = 24;
/// Actions decided and not yet taken, at most (`CHORUS_CONTROLS_MAX_ACTIONS`).
pub const MAX_ACTIONS: usize = 8;
/// The longest room, group or join target, in bytes (a v2 short text).
pub const MAX_NAME: usize = v2::catalog::MAX_SHORT_TEXT;

/// The speaker classes of K67-K70.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SpeakerClass {
    /// K67: buttons or touch, a status LED, a microphone behind a mute switch.
    Compact,
    /// K68: a hidden pairing button and a rear status light only.
    TwoWay,
    /// K69: a pairing button, a status LED, level and phase knobs.
    Subwoofer,
    /// K70: a pairing button and LED, front buttons. The 2U rack amp (K74) is
    /// this class in a rack case.
    StreamingAmp,
}

impl SpeakerClass {
    /// Every class, in the C enum's order.
    pub const ALL: [SpeakerClass; 4] = [
        SpeakerClass::Compact,
        SpeakerClass::TwoWay,
        SpeakerClass::Subwoofer,
        SpeakerClass::StreamingAmp,
    ];

    /// The class's name, as `chorus_speaker_class_name` gives it.
    pub fn name(self) -> &'static str {
        match self {
            SpeakerClass::Compact => "compact",
            SpeakerClass::TwoWay => "two-way",
            SpeakerClass::Subwoofer => "subwoofer",
            SpeakerClass::StreamingAmp => "streaming-amp",
        }
    }

    /// The class with this name, or `None` for a name that is not a class.
    pub fn from_name(name: &str) -> Option<SpeakerClass> {
        SpeakerClass::ALL.iter().copied().find(|c| c.name() == name)
    }

    /// What the class carries.
    pub fn profile(self) -> Profile {
        let buttons = Input::PlayPause.bit()
            | Input::VolumeUp.bit()
            | Input::VolumeDown.bit()
            | Input::Next.bit()
            | Input::Previous.bit();
        match self {
            SpeakerClass::Compact => Profile {
                speaker_class: self,
                inputs: buttons | Input::MicMuteSwitch.bit(),
                led: LedKind::Status,
                has_microphone: true,
                led_follows_visualizer: true,
            },
            SpeakerClass::TwoWay => Profile {
                speaker_class: self,
                inputs: Input::Pairing.bit(),
                led: LedKind::RearStatus,
                has_microphone: false,
                led_follows_visualizer: false,
            },
            SpeakerClass::Subwoofer => Profile {
                speaker_class: self,
                inputs: Input::Pairing.bit()
                    | Input::SubLevelKnob.bit()
                    | Input::SubPhaseKnob.bit(),
                led: LedKind::Status,
                has_microphone: false,
                led_follows_visualizer: true,
            },
            SpeakerClass::StreamingAmp => Profile {
                speaker_class: self,
                inputs: buttons | Input::Pairing.bit(),
                led: LedKind::Status,
                has_microphone: false,
                led_follows_visualizer: true,
            },
        }
    }
}

/// A control. Knobs are read as ADC codes through [`Controls::knob`], every
/// other input as a level through [`Controls::level`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Input {
    /// Play/pause; held, join or leave.
    PlayPause,
    /// Volume up.
    VolumeUp,
    /// Volume down.
    VolumeDown,
    /// Next.
    Next,
    /// Previous.
    Previous,
    /// The pairing button.
    Pairing,
    /// A latching switch, not a button: its level IS the mute state.
    MicMuteSwitch,
    /// The subwoofer's level knob.
    SubLevelKnob,
    /// The subwoofer's phase knob.
    SubPhaseKnob,
}

impl Input {
    /// Every input, in the C enum's order (the order a poll visits them in).
    pub const ALL: [Input; 9] = [
        Input::PlayPause,
        Input::VolumeUp,
        Input::VolumeDown,
        Input::Next,
        Input::Previous,
        Input::Pairing,
        Input::MicMuteSwitch,
        Input::SubLevelKnob,
        Input::SubPhaseKnob,
    ];

    fn index(self) -> usize {
        self as usize
    }

    fn bit(self) -> u32 {
        1u32 << self.index()
    }

    /// The input's name, as `chorus_control_input_name` gives it.
    pub fn name(self) -> &'static str {
        match self {
            Input::PlayPause => "play-pause",
            Input::VolumeUp => "volume-up",
            Input::VolumeDown => "volume-down",
            Input::Next => "next",
            Input::Previous => "previous",
            Input::Pairing => "pairing",
            Input::MicMuteSwitch => "mic-mute-switch",
            Input::SubLevelKnob => "sub-level-knob",
            Input::SubPhaseKnob => "sub-phase-knob",
        }
    }

    /// The input with this name.
    pub fn from_name(name: &str) -> Option<Input> {
        Input::ALL.iter().copied().find(|i| i.name() == name)
    }

    fn is_knob(self) -> bool {
        matches!(self, Input::SubLevelKnob | Input::SubPhaseKnob)
    }
}

/// Which light a class has.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LedKind {
    /// No light.
    None,
    /// A status LED a listener sees (compact, subwoofer, streaming amp).
    Status,
    /// The two-way's rear status light (K68): status only, never the
    /// visualizer.
    RearStatus,
}

/// What a class carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Profile {
    /// The class.
    pub speaker_class: SpeakerClass,
    /// Bit (1 << input) for every input the class has.
    pub inputs: u32,
    /// Its light.
    pub led: LedKind,
    /// Whether it has a microphone.
    pub has_microphone: bool,
    /// Whether its light follows the visualizer while playing.
    pub led_follows_visualizer: bool,
}

impl Profile {
    /// Whether the class has `input`.
    pub fn has(&self, input: Input) -> bool {
        self.inputs & input.bit() != 0
    }
}

/// What an action is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActionKind {
    /// A `controller_command` for the server.
    Command(ControllerCommand),
    /// The pairing button: a local event until goal 14 (adoption) decides
    /// what a press asks for (K92).
    Pairing,
    /// The microphone mute switch moved: true muted, false live.
    MicMute(bool),
    /// A local sound setting: the sub level in tenths of a dB (-120 to 0).
    SubLevel(i32),
    /// A local sound setting: the sub phase in degrees.
    SubPhase(i32),
    /// A long press asked to join a group and no join target is configured:
    /// nothing is sent, and the reason is surfaced instead of guessed.
    NoJoinTarget,
}

/// One thing the controls decided.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Action {
    /// What was decided.
    pub kind: ActionKind,
    /// The input that decided it.
    pub input: Input,
    /// The monotonic time it was decided at (when the level settled, not
    /// when it was polled).
    pub at_ns: u64,
}

impl Action {
    /// The v2 frame for a command action, or `None` for a local event.
    pub fn encode(&self) -> Option<Vec<u8>> {
        match &self.kind {
            ActionKind::Command(c) => v2::encode(&Message::ControllerCommand(c.clone())).ok(),
            _ => None,
        }
    }
}

/// An input the class does not have, or a knob code out of range: counted
/// and never acted on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InputRefused;

#[derive(Debug, Clone, Copy, Default)]
struct InputState {
    raw: bool,
    raw_since_ns: u64,
    stable: bool,
    seen: bool,
    pressed_at_ns: u64,
    long_fired: bool,
    next_repeat_ns: u64,
    knob_code: u32,
    knob_step: i32,
}

/// One endpoint's controls (`chorus_controls_t`).
#[derive(Debug, Clone)]
pub struct Controls {
    profile: Profile,
    inputs: [InputState; 9],
    room: String,
    group: String,
    join_target: String,
    state_known: bool,
    mic_live: bool,
    sub_level_tenths_db: i32,
    sub_phase_deg: i32,
    refused_inputs: u32,
    pending: Vec<Action>,
    dropped_actions: u32,
}

impl Controls {
    /// `room` is this endpoint's own room (leave returns it there);
    /// `join_target` is the group a long press of play/pause joins, or "" for
    /// none. `None` for a name longer than a v2 short text.
    pub fn new(speaker_class: SpeakerClass, room: &str, join_target: &str) -> Option<Controls> {
        if room.len() > MAX_NAME || join_target.len() > MAX_NAME {
            return None;
        }
        Some(Controls {
            profile: speaker_class.profile(),
            inputs: [InputState::default(); 9],
            room: room.to_string(),
            // Until the server says otherwise, the endpoint plays its own room.
            group: room.to_string(),
            join_target: join_target.to_string(),
            state_known: false,
            // The microphone gate starts CLOSED: until the switch has been
            // read, the endpoint does not know it has not been muted.
            mic_live: false,
            sub_level_tenths_db: 0,
            sub_phase_deg: 0,
            refused_inputs: 0,
            pending: Vec::with_capacity(MAX_ACTIONS),
            dropped_actions: 0,
        })
    }

    /// The class's profile.
    pub fn profile(&self) -> &Profile {
        &self.profile
    }

    /// Inputs refused so far.
    pub fn refused_inputs(&self) -> u32 {
        self.refused_inputs
    }

    /// Actions dropped because more than [`MAX_ACTIONS`] waited.
    pub fn dropped_actions(&self) -> u32 {
        self.dropped_actions
    }

    /// The group the server last said this endpoint plays in.
    pub fn group(&self) -> &str {
        &self.group
    }

    /// Whether a `controller_state` has been heard.
    pub fn state_known(&self) -> bool {
        self.state_known
    }

    /// The sub level last set by its knob, in tenths of a dB.
    pub fn sub_level_tenths_db(&self) -> i32 {
        self.sub_level_tenths_db
    }

    /// The sub phase last set by its knob, in degrees.
    pub fn sub_phase_deg(&self) -> i32 {
        self.sub_phase_deg
    }

    fn push(&mut self, action: Action) {
        if self.pending.len() >= MAX_ACTIONS {
            self.dropped_actions += 1;
            return;
        }
        self.pending.push(action);
    }

    fn push_simple(&mut self, kind: ActionKind, input: Input, at_ns: u64) {
        self.push(Action { kind, input, at_ns });
    }

    fn push_command(
        &mut self,
        input: Input,
        command: v2::Command,
        value: i16,
        target: &str,
        at_ns: u64,
    ) {
        self.push(Action {
            kind: ActionKind::Command(ControllerCommand {
                command,
                value,
                target: target.to_string(),
            }),
            input,
            at_ns,
        });
    }

    /// A button or switch level (true pressed or muted), sampled at `now_ns`.
    pub fn level(&mut self, input: Input, level: bool, now_ns: u64) -> Result<(), InputRefused> {
        if !self.profile.has(input) || input.is_knob() {
            self.refused_inputs += 1;
            return Err(InputRefused);
        }
        let s = &mut self.inputs[input.index()];
        if !s.seen || level != s.raw {
            s.raw = level;
            s.raw_since_ns = now_ns;
            s.seen = true;
        }
        Ok(())
    }

    /// Leave when grouped with other rooms, else join the configured target.
    fn long_press_group(&mut self, now_ns: u64) {
        if self.group != self.room {
            self.push_command(Input::PlayPause, v2::Command::Leave, 0, "", now_ns);
        } else if !self.join_target.is_empty() {
            let target = self.join_target.clone();
            self.push_command(Input::PlayPause, v2::Command::Join, 0, &target, now_ns);
        } else {
            self.push_simple(ActionKind::NoJoinTarget, Input::PlayPause, now_ns);
        }
    }

    fn volume_value(input: Input) -> i16 {
        if input == Input::VolumeUp {
            VOLUME_STEP
        } else {
            -VOLUME_STEP
        }
    }

    fn on_press(&mut self, input: Input, at_ns: u64) {
        {
            let s = &mut self.inputs[input.index()];
            s.pressed_at_ns = at_ns;
            s.long_fired = false;
        }
        match input {
            Input::VolumeUp | Input::VolumeDown => {
                self.push_command(
                    input,
                    v2::Command::VolumeStep,
                    Controls::volume_value(input),
                    "",
                    at_ns,
                );
                self.inputs[input.index()].next_repeat_ns = at_ns + REPEAT_DELAY_NS;
            }
            Input::Next => self.push_command(input, v2::Command::Next, 0, "", at_ns),
            Input::Previous => self.push_command(input, v2::Command::Previous, 0, "", at_ns),
            Input::Pairing => self.push_simple(ActionKind::Pairing, input, at_ns),
            // play/pause decides on release or at the long-press mark.
            _ => {}
        }
    }

    fn on_release(&mut self, input: Input, at_ns: u64) {
        if input == Input::PlayPause && !self.inputs[input.index()].long_fired {
            self.push_command(input, v2::Command::Toggle, 0, "", at_ns);
        }
    }

    /// Advance to `now_ns`: settle debounced levels, fire long presses and
    /// repeats, and hand back up to `max` actions in the order they were
    /// decided.
    pub fn poll(&mut self, now_ns: u64, max: usize) -> Vec<Action> {
        for input in Input::ALL {
            let s = self.inputs[input.index()];
            if !self.profile.has(input) || !s.seen || input.is_knob() {
                continue;
            }
            // A level is believed once it has held for the debounce time; the
            // event is dated when it became stable, not when it was polled.
            let settled_at = s.raw_since_ns + DEBOUNCE_NS;
            if s.raw != s.stable && now_ns >= settled_at {
                let stable = s.raw;
                self.inputs[input.index()].stable = stable;
                if input == Input::MicMuteSwitch {
                    self.mic_live = !stable;
                    self.push_simple(ActionKind::MicMute(stable), input, settled_at);
                } else if stable {
                    self.on_press(input, settled_at);
                } else {
                    self.on_release(input, settled_at);
                }
            } else if input == Input::MicMuteSwitch
                && !s.raw
                && !s.stable
                && !self.mic_live
                && now_ns >= settled_at
            {
                // The first stable reading of a switch in the live position
                // opens the gate.
                self.mic_live = true;
                self.push_simple(ActionKind::MicMute(false), input, settled_at);
            }
            let s = self.inputs[input.index()];
            if !s.stable {
                continue;
            }
            if input == Input::PlayPause && !s.long_fired && now_ns >= s.pressed_at_ns + LONG_PRESS_NS
            {
                self.inputs[input.index()].long_fired = true;
                self.long_press_group(s.pressed_at_ns + LONG_PRESS_NS);
            }
            if matches!(input, Input::VolumeUp | Input::VolumeDown) {
                while now_ns >= self.inputs[input.index()].next_repeat_ns {
                    let at = self.inputs[input.index()].next_repeat_ns;
                    self.push_command(
                        input,
                        v2::Command::VolumeStep,
                        Controls::volume_value(input),
                        "",
                        at,
                    );
                    self.inputs[input.index()].next_repeat_ns += REPEAT_PERIOD_NS;
                }
            }
        }
        let n = self.pending.len().min(max);
        self.pending.drain(..n).collect()
    }

    /// A knob's ADC code, 0 to [`KNOB_MAX`].
    pub fn knob(&mut self, input: Input, code: u32, now_ns: u64) -> Result<(), InputRefused> {
        if !input.is_knob() || !self.profile.has(input) || code > KNOB_MAX {
            self.refused_inputs += 1;
            return Err(InputRefused);
        }
        let level = input == Input::SubLevelKnob;
        let steps = if level {
            (-SUB_LEVEL_MIN_TENTHS_DB / SUB_LEVEL_STEP_TENTHS_DB) + 1
        } else {
            (SUB_PHASE_MAX_DEG / SUB_PHASE_STEP_DEG) + 1
        };
        let s = &mut self.inputs[input.index()];
        let step = knob_step(code, steps, s.knob_step, s.seen);
        let changed = !s.seen || step != s.knob_step;
        s.seen = true;
        s.knob_code = code;
        s.knob_step = step;
        if !changed {
            return Ok(());
        }
        if level {
            // Step 0 is the full cut, the last step 0 dB: never above 0 dB.
            let tenths = (SUB_LEVEL_MIN_TENTHS_DB + step * SUB_LEVEL_STEP_TENTHS_DB).min(0);
            self.sub_level_tenths_db = tenths;
            self.push_simple(ActionKind::SubLevel(tenths), input, now_ns);
        } else {
            let deg = step * SUB_PHASE_STEP_DEG;
            self.sub_phase_deg = deg;
            self.push_simple(ActionKind::SubPhase(deg), input, now_ns);
        }
        Ok(())
    }

    /// The server's `controller_state`, which join and leave are decided
    /// against. A group longer than a short text is ignored.
    pub fn state(&mut self, state: &ControllerState) {
        if state.group.len() > MAX_NAME {
            return;
        }
        self.group = if state.group.is_empty() {
            self.room.clone()
        } else {
            state.group.clone()
        };
        self.state_known = true;
    }

    /// Whether microphone audio may leave the endpoint now.
    pub fn mic_live(&self) -> bool {
        self.profile.has_microphone && self.mic_live
    }

    /// The only way microphone samples reach the voice path: copies `input`
    /// to `out` and returns the count while the gate is open; returns 0 and
    /// copies nothing while it is closed.
    pub fn mic_pass(&self, input: &[i16], out: &mut [i16]) -> usize {
        if !self.mic_live() {
            return 0;
        }
        let n = input.len().min(out.len());
        out[..n].copy_from_slice(&input[..n]);
        n
    }
}

/// The step a knob code maps to, with hysteresis: the code has to go past
/// the step's edge by [`KNOB_HYSTERESIS`] codes before the step changes.
fn knob_step(code: u32, steps: i32, current: i32, have_current: bool) -> i32 {
    let span = KNOB_MAX as i32 + 1;
    let step = ((i64::from(code) * i64::from(steps)) / i64::from(span)) as i32;
    let step = step.min(steps - 1);
    if !have_current || step == current {
        return step;
    }
    let edge = if step > current {
        ((i64::from(current + 1) * i64::from(span)) / i64::from(steps)) as i32
    } else {
        ((i64::from(current) * i64::from(span)) / i64::from(steps)) as i32
    };
    if (code as i32 - edge).abs() >= KNOB_HYSTERESIS as i32 {
        step
    } else {
        current
    }
}
