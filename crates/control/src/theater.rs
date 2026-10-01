//! A room's TV settings (catalog v2, goal 13): its signed A/V trim and its
//! `tv_upmix`, and the one piece of arithmetic the TV relay needs from them.
//!
//! # The A/V trim
//!
//! `av_trim_ms` is how much later (positive) or earlier (negative) a room
//! hears a TV input than chorus's own TV latency would put it, so a person can
//! line the sound up with the picture the TV draws. Positive delays the audio:
//! a TV whose picture path is slower than its audio needs the audio held back.
//! The bounds are ASSUMED (the goal-13 design envelope): -100 to +200 ms,
//! wider on the delay side because a TV's picture processing is the usual
//! cause and it only ever makes the picture late. They wait on the Needs item
//! "The three TVs: model, eARC port, optical out and audio menu".
//!
//! The trim applies only to the TV relay's stamps (the integration track):
//! `play_at = capture_stamp + max(L_floor, L_tv + trim)`. Audio cannot be
//! played before it has crossed the network and the endpoints' buffers, so a
//! trim that asks for a lead below the floor is CLAMPED to the floor and
//! reported (`av-trim-clamped`), never refused: the room still plays, as early
//! as it can. [`tv_play_at_lead_ns`] is that rule, pure, so the relay and its
//! tests use one implementation.
//!
//! # `tv_upmix`
//!
//! What a theater set's surround members play when the stream has no surround
//! channel (a stereo TV, every TV path goal 13 builds): `off`, silence (the
//! default, ASSUMED: an upmix plays a stereo mix from speakers its mixer did
//! not use, so it is the room's choice), or `ambient`, the passive matrix
//! surround the endpoint DSP chain derives from `FL - FR` (`docs/dsp.md`, "The
//! theater maps", where its gain, delay and band are cited). It travels to the
//! endpoints in the `sound` message (0x39) beside the room's other sound
//! settings, which is why the catalog carries it in the `sound` command.
//!
//! No clock and no PCM here: integers in, integers out.

use std::fmt;

/// The A/V trim's range, ms, inclusive. ASSUMED (see the module).
pub const AV_TRIM_MS: (i16, i16) = (-100, 200);

/// What a theater set's surround members play from a stream with no surround
/// channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum TvUpmix {
    /// Silence. The default, ASSUMED.
    #[default]
    Off,
    /// The passive matrix surround, from `FL - FR`.
    Ambient,
}

impl TvUpmix {
    /// The catalog's word for it.
    pub fn name(self) -> &'static str {
        match self {
            TvUpmix::Off => "off",
            TvUpmix::Ambient => "ambient",
        }
    }

    /// Read the catalog's word back.
    pub fn parse(text: &str) -> Option<TvUpmix> {
        [TvUpmix::Off, TvUpmix::Ambient]
            .into_iter()
            .find(|u| u.name() == text)
    }

    /// Its number on the audio wire (`sound`'s `tv_upmix`, `docs/protocol.md`).
    pub fn wire(self) -> u8 {
        match self {
            TvUpmix::Off => 0,
            TvUpmix::Ambient => 1,
        }
    }
}

impl fmt::Display for TvUpmix {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// The log word the TV relay prints when a trim asked for less than the floor.
pub const AV_TRIM_CLAMPED: &str = "av-trim-clamped";

/// The stamp lead the TV relay gives a captured frame: `l_tv_ns` (the room's
/// TV latency) moved by `trim_ms` (positive delays the audio), never below
/// `l_floor_ns` (the least lead the network and the endpoints' buffers can
/// carry). Returns the lead and whether the floor clamped it; a clamp is
/// reported by the caller as [`AV_TRIM_CLAMPED`] and is never a refusal.
///
/// Saturating: no input wraps, and a negative lead is impossible because the
/// floor is not negative. A `trim_ms` outside [`AV_TRIM_MS`] is not the
/// catalog's (it refuses one) but is still computed as asked.
pub fn tv_play_at_lead_ns(l_tv_ns: u64, l_floor_ns: u64, trim_ms: i16) -> (u64, bool) {
    let trim_ns = i128::from(trim_ms) * 1_000_000;
    let asked = i128::from(l_tv_ns) + trim_ns;
    let floor = i128::from(l_floor_ns);
    if asked < floor {
        (l_floor_ns, true)
    } else {
        (u64::try_from(asked).unwrap_or(u64::MAX), false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MS: u64 = 1_000_000;

    #[test]
    fn a_trim_moves_the_lead_and_the_floor_clamps_it() {
        // No trim: the TV latency itself.
        assert_eq!(tv_play_at_lead_ns(30 * MS, 12 * MS, 0), (30 * MS, false));
        // Positive delays the audio.
        assert_eq!(tv_play_at_lead_ns(30 * MS, 12 * MS, 200), (230 * MS, false));
        // Negative brings it earlier, down to the floor exactly.
        assert_eq!(tv_play_at_lead_ns(30 * MS, 12 * MS, -18), (12 * MS, false));
        // Past the floor: clamped and reported, never refused.
        assert_eq!(tv_play_at_lead_ns(30 * MS, 12 * MS, -19), (12 * MS, true));
        assert_eq!(tv_play_at_lead_ns(30 * MS, 12 * MS, -100), (12 * MS, true));
        // Saturating at the top, never wrapping.
        assert_eq!(tv_play_at_lead_ns(u64::MAX, 0, 200), (u64::MAX, false));
        assert_eq!(tv_play_at_lead_ns(0, 0, i16::MIN), (0, true));
    }

    #[test]
    fn tv_upmix_spells_one_way() {
        for u in [TvUpmix::Off, TvUpmix::Ambient] {
            assert_eq!(TvUpmix::parse(u.name()), Some(u));
        }
        assert_eq!(TvUpmix::parse("Ambient"), None);
        assert_eq!(TvUpmix::default(), TvUpmix::Off);
        assert_eq!(TvUpmix::Ambient.wire(), 1);
    }
}
