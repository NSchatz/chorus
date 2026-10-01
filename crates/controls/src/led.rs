//! The status LED: its state by priority, its fixed palette, and following
//! the visualizer while playing. Line for line the LED half of
//! `firmware/src/controls.c`.

use chorus_protocol::v2::Message;

use crate::model::{LedKind, Profile};

/// ASSUMED: a visualizer stream older than this is over; the LED returns to
/// its steady playing colour.
pub const VISUALIZER_STALE_NS: u64 = 500_000_000;
/// ASSUMED: a beat at least this strong flashes the LED to full brightness.
pub const BEAT_THRESHOLD: u8 = 128;
/// Frames the LED holds ahead of the moment they are heard.
pub const QUEUE: usize = 16;

/// What the LED shows, one at a time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LedState {
    /// Starting up.
    Boot,
    /// No link to the server.
    LinkDown,
    /// Up and not playing.
    Idle,
    /// Playing.
    Playing,
    /// The microphone is muted.
    Muted,
    /// Pairing.
    Pairing,
    /// A fault.
    Fault,
}

impl LedState {
    /// Every state, in the C enum's order.
    pub const ALL: [LedState; 7] = [
        LedState::Boot,
        LedState::LinkDown,
        LedState::Idle,
        LedState::Playing,
        LedState::Muted,
        LedState::Pairing,
        LedState::Fault,
    ];

    /// The state's name, as `chorus_led_state_name` gives it.
    pub fn name(self) -> &'static str {
        match self {
            LedState::Boot => "boot",
            LedState::LinkDown => "link-down",
            LedState::Idle => "idle",
            LedState::Playing => "playing",
            LedState::Muted => "muted",
            LedState::Pairing => "pairing",
            LedState::Fault => "fault",
        }
    }

    /// The state with this name.
    pub fn from_name(name: &str) -> Option<LedState> {
        LedState::ALL.iter().copied().find(|s| s.name() == name)
    }

    /// The fixed palette, ASSUMED (`docs/hardware/controls.md`): dim, since a
    /// status light in a living room should not be a lamp.
    pub fn palette(self) -> LedOutput {
        let (red, green, blue, brightness) = match self {
            LedState::Boot => (255, 255, 255, 32),
            LedState::LinkDown => (255, 160, 0, 48),
            LedState::Idle => (255, 255, 255, 8),
            LedState::Playing => (255, 255, 255, 24),
            LedState::Muted => (255, 64, 0, 48),
            LedState::Pairing => (0, 96, 255, 64),
            LedState::Fault => (255, 0, 0, 96),
        };
        LedOutput {
            red,
            green,
            blue,
            brightness,
        }
    }
}

/// What the endpoint knows, from which the LED's state is decided.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LedInputs {
    /// Booted.
    pub booted: bool,
    /// The link to the server is up.
    pub link_up: bool,
    /// Adopted by a server.
    pub adopted: bool,
    /// Playing.
    pub playing: bool,
    /// The microphone is muted.
    pub mic_muted: bool,
    /// Pairing.
    pub pairing: bool,
    /// A fault.
    pub fault: bool,
}

impl LedInputs {
    /// The state these inputs mean: a fault first, then pairing, then a muted
    /// microphone, then boot, then the link, then playing, else idle.
    pub fn decide(&self) -> LedState {
        if self.fault {
            LedState::Fault
        } else if self.pairing {
            LedState::Pairing
        } else if self.mic_muted {
            LedState::Muted
        } else if !self.booted {
            LedState::Boot
        } else if !self.link_up {
            LedState::LinkDown
        } else if self.playing && self.adopted {
            LedState::Playing
        } else {
            LedState::Idle
        }
    }
}

/// A colour and a brightness.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LedOutput {
    /// Red.
    pub red: u8,
    /// Green.
    pub green: u8,
    /// Blue.
    pub blue: u8,
    /// Brightness, 0 off to 255.
    pub brightness: u8,
}

#[derive(Debug, Clone, Copy)]
struct Event {
    timestamp_ns: u64,
    colour: Option<LedOutput>,
    beat: u8,
    peak: u8,
}

/// One LED (`chorus_led_t`).
#[derive(Debug, Clone)]
pub struct Led {
    kind: LedKind,
    follows_visualizer: bool,
    queue: Vec<Event>,
    dropped: u32,
    base: LedOutput,
    have_frame: bool,
    frame_at_ns: u64,
    beat: u8,
    peak: u8,
}

impl Led {
    /// The LED of a class.
    pub fn new(profile: &Profile) -> Led {
        Led {
            kind: profile.led,
            follows_visualizer: profile.led_follows_visualizer && profile.led == LedKind::Status,
            queue: Vec::with_capacity(QUEUE),
            dropped: 0,
            base: LedState::Playing.palette(),
            have_frame: false,
            frame_at_ns: 0,
            beat: 0,
            peak: 0,
        }
    }

    /// Frames refused because the queue was full.
    pub fn dropped(&self) -> u32 {
        self.dropped
    }

    /// Queue a `visualizer_frame` or `color`; false for any other message or
    /// a full queue. Frames are held until they are heard: their timestamps
    /// are on the server timeline.
    pub fn offer(&mut self, message: &Message) -> bool {
        let e = match message {
            Message::VisualizerFrame(v) => Event {
                timestamp_ns: v.timestamp_ns,
                colour: None,
                beat: v.beat,
                peak: v.peak,
            },
            Message::Color(c) => Event {
                timestamp_ns: c.timestamp_ns,
                colour: Some(LedOutput {
                    red: c.red,
                    green: c.green,
                    blue: c.blue,
                    brightness: c.brightness,
                }),
                beat: 0,
                peak: 0,
            },
            _ => return false,
        };
        if self.queue.len() >= QUEUE {
            self.dropped += 1;
            return false;
        }
        // Keep the queue in the order the events are heard.
        let at = self
            .queue
            .iter()
            .rposition(|q| q.timestamp_ns <= e.timestamp_ns)
            .map(|i| i + 1)
            .unwrap_or(0);
        self.queue.insert(at, e);
        true
    }

    /// What the LED shows at `server_now_ns` in `state`.
    pub fn render(&mut self, state: LedState, server_now_ns: u64) -> LedOutput {
        let taken = self
            .queue
            .iter()
            .take_while(|e| e.timestamp_ns <= server_now_ns)
            .count();
        for e in self.queue.drain(..taken) {
            match e.colour {
                Some(c) => self.base = c,
                None => {
                    self.have_frame = true;
                    self.frame_at_ns = e.timestamp_ns;
                    self.beat = e.beat;
                    self.peak = e.peak;
                }
            }
        }
        if self.kind == LedKind::None {
            return LedOutput::default();
        }
        if state != LedState::Playing
            || !self.follows_visualizer
            || !self.have_frame
            || server_now_ns.wrapping_sub(self.frame_at_ns) > VISUALIZER_STALE_NS
        {
            return state.palette();
        }
        let mut out = self.base;
        if self.beat < BEAT_THRESHOLD {
            out.brightness =
                ((u32::from(self.base.brightness) * u32::from(self.peak) + 127) / 255) as u8;
        }
        out
    }
}
