//! The TV's power as CEC sees it, shared with the source role, and the rule
//! that turns it and the captured audio into the TV input's offered signal.
//!
//! The CEC thread writes [`TvPower`]; the source role's thread reads it once
//! per captured chunk through [`TvSignal`]. Nothing here reads a clock: the
//! source role calls [`TvSignal::update`] at its own cadence, so "at once"
//! means "at the next captured chunk", 20 ms at most on today's line-in
//! (`crates/client-linux/src/config.rs`, `LINE_IN_CHUNK_MS`).

use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};

/// What CEC last said about the TV's power.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TvPowerState {
    /// Nothing heard yet.
    Unknown = 0,
    /// On (or turning on).
    On = 1,
    /// In standby (or going to it).
    Standby = 2,
}

impl TvPowerState {
    /// The status-line word.
    pub fn name(self) -> &'static str {
        match self {
            TvPowerState::Unknown => "unknown",
            TvPowerState::On => "on",
            TvPowerState::Standby => "standby",
        }
    }

    fn from_u8(v: u8) -> TvPowerState {
        match v {
            1 => TvPowerState::On,
            2 => TvPowerState::Standby,
            _ => TvPowerState::Unknown,
        }
    }
}

/// The TV's power, shared between threads.
#[derive(Debug, Default)]
pub struct TvPower {
    state: AtomicU8,
    changes: AtomicU64,
}

impl TvPower {
    /// Unknown, no changes.
    pub fn new() -> TvPower {
        TvPower::default()
    }

    /// Record `state`; true when it differs from the last one.
    pub fn set(&self, state: TvPowerState) -> bool {
        let before = self.state.swap(state as u8, Ordering::SeqCst);
        if before != state as u8 {
            self.changes.fetch_add(1, Ordering::SeqCst);
            true
        } else {
            false
        }
    }

    /// The state now.
    pub fn get(&self) -> TvPowerState {
        TvPowerState::from_u8(self.state.load(Ordering::SeqCst))
    }

    /// How many times it has changed.
    pub fn changes(&self) -> u64 {
        self.changes.load(Ordering::SeqCst)
    }
}

/// Why the offered signal is what it is, for the status line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalReason {
    /// Audio is present on the input.
    Audio,
    /// No audio, and nothing else says there is a signal.
    Quiet,
    /// No audio yet, but CEC says the TV turned on (autoplay on power).
    TvOn,
    /// CEC says the TV went to standby: the signal ends at once.
    Standby,
}

impl SignalReason {
    /// The `reason=` word.
    pub fn name(self) -> &'static str {
        match self {
            SignalReason::Audio => "audio",
            SignalReason::Quiet => "quiet",
            SignalReason::TvOn => "tv-on",
            SignalReason::Standby => "standby",
        }
    }
}

/// The TV input's offered signal: audio present OR (the TV is on AND
/// autoplay on power), except that a standby ends it at once (design
/// envelope section 3, goal 13).
///
/// "At once" overrides the audio detector's 2 s hold
/// (`crates/client-linux/src/source.rs`, `SIGNAL_OFF_HOLD_MS`): after a
/// standby the audio that the detector still holds as present does not count
/// until the detector has let it go and found audio again, or the TV is seen
/// on again. So a TV that keeps its optical output alive in standby, or a
/// late tail of sound, cannot hold the room after the TV turned off.
#[derive(Debug)]
pub struct TvSignal<P> {
    power: P,
    autoplay_on_power: bool,
    audio: bool,
    after_standby: bool,
    offered: bool,
    seen: u64,
}

impl<P: std::ops::Deref<Target = TvPower>> TvSignal<P> {
    /// Start from "nothing offered", as the session's first offer says.
    pub fn new(power: P, autoplay_on_power: bool) -> TvSignal<P> {
        TvSignal {
            power,
            autoplay_on_power,
            audio: false,
            after_standby: false,
            offered: false,
            seen: u64::MAX,
        }
    }

    /// Take the audio detector's edge for this chunk (`None` when it did not
    /// change) and the TV's power now; return the new offer when the offered
    /// signal changes.
    pub fn update(&mut self, audio_edge: Option<bool>) -> Option<(bool, SignalReason)> {
        if let Some(present) = audio_edge {
            self.audio = present;
            if present {
                self.after_standby = false;
            }
        }
        let changes = self.power.changes();
        let power = self.power.get();
        if changes != self.seen {
            self.seen = changes;
            match power {
                TvPowerState::Standby => self.after_standby = true,
                TvPowerState::On => self.after_standby = false,
                TvPowerState::Unknown => {}
            }
        }
        let tv_on = power == TvPowerState::On && self.autoplay_on_power;
        let wanted = !self.after_standby && (self.audio || tv_on);
        if wanted == self.offered {
            return None;
        }
        self.offered = wanted;
        let reason = match (wanted, self.audio) {
            (true, true) => SignalReason::Audio,
            (true, false) => SignalReason::TvOn,
            (false, _) if self.after_standby => SignalReason::Standby,
            (false, _) => SignalReason::Quiet,
        };
        Some((wanted, reason))
    }

    /// What is offered now.
    pub fn offered(&self) -> bool {
        self.offered
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn audio_alone_is_the_signal_as_before() {
        let p = Arc::new(TvPower::new());
        let mut s = TvSignal::new(Arc::clone(&p), true);
        assert_eq!(s.update(None), None);
        assert_eq!(s.update(Some(true)), Some((true, SignalReason::Audio)));
        assert_eq!(s.update(Some(false)), Some((false, SignalReason::Quiet)));
    }

    #[test]
    fn tv_on_raises_the_signal_with_autoplay_and_not_without() {
        let p = Arc::new(TvPower::new());
        let mut on = TvSignal::new(Arc::clone(&p), true);
        let mut off = TvSignal::new(Arc::clone(&p), false);
        p.set(TvPowerState::On);
        assert_eq!(on.update(None), Some((true, SignalReason::TvOn)));
        assert_eq!(off.update(None), None);
        // Audio arriving changes nothing offered; it going does not end a
        // signal the TV's power still holds.
        assert_eq!(on.update(Some(true)), None);
        assert_eq!(on.update(Some(false)), None);
        assert!(on.offered());
    }

    #[test]
    fn standby_ends_the_signal_at_once_even_while_the_detector_holds_audio() {
        let p = Arc::new(TvPower::new());
        let mut s = TvSignal::new(Arc::clone(&p), true);
        p.set(TvPowerState::On);
        assert_eq!(s.update(Some(true)), Some((true, SignalReason::Audio)));
        p.set(TvPowerState::Standby);
        assert_eq!(s.update(None), Some((false, SignalReason::Standby)));
        // The detector lets the held audio go: nothing new is offered.
        assert_eq!(s.update(Some(false)), None);
        // Audio found again (some other device on the TV's output) counts.
        assert_eq!(s.update(Some(true)), Some((true, SignalReason::Audio)));
    }

    #[test]
    fn the_tv_seen_on_again_after_standby_raises_the_signal() {
        let p = Arc::new(TvPower::new());
        let mut s = TvSignal::new(Arc::clone(&p), true);
        p.set(TvPowerState::On);
        s.update(None);
        p.set(TvPowerState::Standby);
        assert_eq!(s.update(None), Some((false, SignalReason::Standby)));
        p.set(TvPowerState::On);
        assert_eq!(s.update(None), Some((true, SignalReason::TvOn)));
    }
}
