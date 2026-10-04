//! Per-room sound (catalog v2, goal 12): tone, loudness, night mode and
//! speech enhancement; bass management for a bonded set with a sub; and the
//! room-correction EQ. What a room SOUNDS like, as settings; the DSP that
//! realises them runs on the endpoints (`crates/dsp`, `firmware/src/dsp.c`),
//! which receive these values on the audio wire as `sound` (0x39,
//! `docs/protocol.md`).
//!
//! # One spelling, as everything in the catalog
//!
//! The decibel and Q values are DECIMALS with a fixed number of places, held
//! as integers ([`FixedPoint`]): a gain is hundredths of a dB and a Q is
//! thousandths, which are exactly the units the wire carries (`gain_cdb`,
//! `q_milli`, `sub_level_cdb`), so a value crosses from the catalog to the
//! audio wire with no conversion and no binary floating point anywhere. A
//! value is written with all of its places (`-3.50`, `0.707`) and read back
//! with at most that many; `0.7071` is refused, not rounded, for the reason
//! [`crate::catalog::Volume`] gives.
//!
//! # The room-correction bounds are THE bounds
//!
//! [`ROOM_EQ_MAX_FILTERS`], [`ROOM_EQ_FREQ_HZ`], [`ROOM_EQ_GAIN_CDB`] and
//! [`ROOM_EQ_Q_MILLI`] are what the catalog accepts, what the wire accepts
//! (the C mirror is `CHORUS_DSP_ROOM_EQ_*` and `CHORUS_V2_SOUND_*`) and what
//! the room-correction fitter may emit. Cut-heavy on purpose (+3 dB at most,
//! -12 dB at least): a room's peaks are fixed by cutting them, and a boost
//! into a null spends headroom and amplifier power on a cancellation no EQ
//! can fill. Below 1 kHz only, where a room's modes dominate and one
//! listening position's measurement still says something about the room.
//! The numbers are ASSUMED (the goal-12 design envelope), not measured.

use std::fmt;

use crate::theater::TvUpmix;

/// Most room-correction filters a room holds. ASSUMED: the envelope's bound,
/// enough for the handful of modes below 1 kHz a fit finds in a living room.
pub const ROOM_EQ_MAX_FILTERS: usize = 8;

/// A room-correction filter's centre frequency, in Hz, inclusive.
pub const ROOM_EQ_FREQ_HZ: (u16, u16) = (20, 1_000);

/// A room-correction filter's gain, in hundredths of a dB, inclusive: -12.00
/// to +3.00 dB.
pub const ROOM_EQ_GAIN_CDB: (i16, i16) = (-1_200, 300);

/// A room-correction filter's Q, in thousandths, inclusive: 0.500 to 10.000.
pub const ROOM_EQ_Q_MILLI: (u16, u16) = (500, 10_000);

/// Bass and treble, in whole dB steps, inclusive. ASSUMED: -10 to +10 at
/// 1 dB a step. Sonos's own apps offer -10 to +10 with 0 flat (its support
/// article <https://support.sonos.com/en-us/article/adjust-the-bass-treble-balance-and-loudness>,
/// read 2026-10-01, describes the controls without printing the range, and
/// its developer documentation at docs.sonos.com prints neither), so the
/// range is the envelope's choice, recorded here as not cited.
pub const TONE_DB: (i8, i8) = (-10, 10);

/// A bonded set's crossover between the mains and the sub, in Hz, inclusive.
pub const CROSSOVER_HZ: (u16, u16) = (40, 200);

/// The crossover a room starts with: 80 Hz, the THX/SMPTE crossover
/// (<https://github.com/NSchatz/chorus/blob/535ed2816b5735153ac81c52a235024aa806f2b5/.claude/goals/2026-09-chorus-research/research-theater.md>
/// section 5.2
/// and its source [B1]).
pub const DEFAULT_CROSSOVER_HZ: u16 = 80;

/// The sub's level trim, in hundredths of a dB, inclusive: -12.00 to +6.00.
/// ASSUMED (the envelope's bound).
pub const SUB_LEVEL_CDB: (i16, i16) = (-1_200, 600);

/// A decimal with a fixed number of places, held as an integer count of the
/// smallest place.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct FixedPoint;

impl FixedPoint {
    /// Write `value` (in units of 10^-`places`) with exactly `places`
    /// fractional digits: `literal(-350, 2)` is `-3.50`.
    pub fn literal(value: i64, places: u32) -> String {
        let scale = 10i64.pow(places);
        let sign = if value < 0 { "-" } else { "" };
        let v = value.unsigned_abs();
        let s = scale as u64;
        format!(
            "{}{}.{:0width$}",
            sign,
            v / s,
            v % s,
            width = places as usize
        )
    }

    /// Read a decimal with at most `places` fractional digits back, in units
    /// of 10^-`places`: an optional `-`, digits, and optionally a point and
    /// one to `places` digits. No exponent, no `+`, no leading point, no
    /// negative zero (`-0`, `-0.00`): one value, one spelling.
    pub fn parse(text: &str, places: u32) -> Option<i64> {
        let (negative, body) = match text.strip_prefix('-') {
            Some(rest) => (true, rest),
            None => (false, text),
        };
        let (whole, fraction) = match body.split_once('.') {
            Some((w, f)) => {
                if f.is_empty() {
                    return None;
                }
                (w, f)
            }
            None => (body, ""),
        };
        if whole.is_empty() || whole.len() > 9 || !whole.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        if fraction.len() > places as usize || !fraction.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let mut padded = fraction.to_string();
        while padded.len() < places as usize {
            padded.push('0');
        }
        let whole: i64 = whole.parse().ok()?;
        let fraction: i64 = if padded.is_empty() {
            0
        } else {
            padded.parse().ok()?
        };
        let magnitude = whole
            .checked_mul(10i64.pow(places))?
            .checked_add(fraction)?;
        if negative && magnitude == 0 {
            return None;
        }
        Some(if negative { -magnitude } else { magnitude })
    }
}

/// Tone, loudness, night mode and speech enhancement for one room.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SoundSettings {
    /// Low-shelf gain, whole dB, [`TONE_DB`].
    pub bass: i8,
    /// High-shelf gain, whole dB, [`TONE_DB`].
    pub treble: i8,
    /// Loudness compensation at low volume.
    pub loudness: bool,
    /// Night mode: the dynamic-range compressor.
    pub night: bool,
    /// Speech enhancement.
    pub speech: bool,
    /// (goal 13) What a theater set's surround members play from a stream
    /// with no surround channel ([`crate::theater`]).
    pub tv_upmix: TvUpmix,
}

impl Default for SoundSettings {
    /// Flat, with loudness on and night and speech off. Loudness on is
    /// ASSUMED as Sonos's default: Sonos's own documentation read 2026-10-01
    /// (the support article above and docs.sonos.com) does not state a
    /// default; a Sonos community answer says it defaults to on for current
    /// players
    /// (<https://en.community.sonos.com/components-and-architectural-228996/sonos-amp-default-settings-loudness-and-sub-6821628>,
    /// read 2026-10-01), which is not Sonos's documentation.
    fn default() -> SoundSettings {
        SoundSettings {
            bass: 0,
            treble: 0,
            loudness: true,
            night: false,
            speech: false,
            tv_upmix: TvUpmix::Off,
        }
    }
}

/// Which way the sub's output is wired: as the mains, or inverted (a sub on
/// the far side of a room, or wired backwards, fixed without a screwdriver).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Polarity {
    /// In phase with the mains.
    Normal,
    /// Inverted.
    Inverted,
}

impl Polarity {
    /// The catalog's word for it.
    pub fn name(self) -> &'static str {
        match self {
            Polarity::Normal => "normal",
            Polarity::Inverted => "inverted",
        }
    }

    /// Read the catalog's word back.
    pub fn parse(text: &str) -> Option<Polarity> {
        [Polarity::Normal, Polarity::Inverted]
            .into_iter()
            .find(|p| p.name() == text)
    }
}

impl fmt::Display for Polarity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// Bass management for a room whose bonded set has an `LFE` member: the
/// mains above the crossover, the sub below it, at a level and a polarity.
/// Held whether or not the set has a sub, so a sub added later starts from
/// what was set; the state says whether it is `active`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BassManagement {
    /// The crossover, Hz, [`CROSSOVER_HZ`].
    pub crossover_hz: u16,
    /// The sub's level trim, hundredths of a dB, [`SUB_LEVEL_CDB`].
    pub sub_level_cdb: i16,
    /// The sub's polarity.
    pub sub_polarity: Polarity,
}

impl Default for BassManagement {
    fn default() -> BassManagement {
        BassManagement {
            crossover_hz: DEFAULT_CROSSOVER_HZ,
            sub_level_cdb: 0,
            sub_polarity: Polarity::Normal,
        }
    }
}

/// One room-correction filter: a peaking EQ at `freq_hz`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EqFilter {
    /// Centre frequency, Hz, [`ROOM_EQ_FREQ_HZ`].
    pub freq_hz: u16,
    /// Gain, hundredths of a dB, [`ROOM_EQ_GAIN_CDB`].
    pub gain_cdb: i16,
    /// Q, thousandths, [`ROOM_EQ_Q_MILLI`].
    pub q_milli: u16,
}

impl EqFilter {
    /// Why this filter is outside the room-correction bounds, naming the
    /// field, or `None` when it is inside them.
    pub fn problem(&self) -> Option<String> {
        if !(ROOM_EQ_FREQ_HZ.0..=ROOM_EQ_FREQ_HZ.1).contains(&self.freq_hz) {
            return Some(format!(
                "freq_hz {} is outside the room-correction bounds, {} to {} Hz",
                self.freq_hz, ROOM_EQ_FREQ_HZ.0, ROOM_EQ_FREQ_HZ.1
            ));
        }
        if !(ROOM_EQ_GAIN_CDB.0..=ROOM_EQ_GAIN_CDB.1).contains(&self.gain_cdb) {
            return Some(format!(
                "gain_db {} is outside the room-correction bounds, {} to {} dB (a correction \
                 cuts peaks; it does not fill nulls)",
                FixedPoint::literal(i64::from(self.gain_cdb), 2),
                FixedPoint::literal(i64::from(ROOM_EQ_GAIN_CDB.0), 2),
                FixedPoint::literal(i64::from(ROOM_EQ_GAIN_CDB.1), 2)
            ));
        }
        if !(ROOM_EQ_Q_MILLI.0..=ROOM_EQ_Q_MILLI.1).contains(&self.q_milli) {
            return Some(format!(
                "q {} is outside the room-correction bounds, {} to {}",
                FixedPoint::literal(i64::from(self.q_milli), 3),
                FixedPoint::literal(i64::from(ROOM_EQ_Q_MILLI.0), 3),
                FixedPoint::literal(i64::from(ROOM_EQ_Q_MILLI.1), 3)
            ));
        }
        None
    }

    /// The persisted spelling: `120 -3.50 4.000`.
    pub fn persisted(&self) -> String {
        format!(
            "{} {} {}",
            self.freq_hz,
            FixedPoint::literal(i64::from(self.gain_cdb), 2),
            FixedPoint::literal(i64::from(self.q_milli), 3)
        )
    }

    /// Read the persisted spelling back, held to the bounds.
    pub fn from_persisted(text: &str) -> Result<EqFilter, String> {
        let parts: Vec<&str> = text.split_whitespace().collect();
        let bad = || {
            format!(
                "'{}' is not 'freq_hz gain_db q' (e.g. '120 -3.50 4.000')",
                text
            )
        };
        if parts.len() != 3 {
            return Err(bad());
        }
        let freq: u16 = parts[0].parse().map_err(|_| bad())?;
        if freq.to_string() != parts[0] {
            return Err(bad());
        }
        let gain = FixedPoint::parse(parts[1], 2)
            .and_then(|g| i16::try_from(g).ok())
            .ok_or_else(bad)?;
        let q = FixedPoint::parse(parts[2], 3)
            .and_then(|q| u16::try_from(q).ok())
            .ok_or_else(bad)?;
        let filter = EqFilter {
            freq_hz: freq,
            gain_cdb: gain,
            q_milli: q,
        };
        match filter.problem() {
            Some(p) => Err(p),
            None => Ok(filter),
        }
    }
}

/// A room's correction EQ: up to [`ROOM_EQ_MAX_FILTERS`] filters, and
/// whether they are applied. Disabling keeps the filters, so a person can
/// compare with and without a fit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoomEq {
    /// Whether the filters are applied. ASSUMED default true: a room with no
    /// filters is flat either way, and a fit someone just made is meant to
    /// be heard.
    pub enabled: bool,
    /// The filters, in the order given.
    pub filters: Vec<EqFilter>,
}

impl Default for RoomEq {
    fn default() -> RoomEq {
        RoomEq {
            enabled: true,
            filters: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fixed_point_value_has_one_spelling() {
        for (text, places, value) in [
            ("-3.50", 2, -350i64),
            ("0.00", 2, 0),
            ("3.00", 2, 300),
            ("-12.00", 2, -1200),
            ("0.707", 3, 707),
            ("10.000", 3, 10_000),
        ] {
            assert_eq!(FixedPoint::parse(text, places), Some(value), "{}", text);
            assert_eq!(FixedPoint::literal(value, places), text);
        }
        assert_eq!(FixedPoint::parse("-3.5", 2), Some(-350), "fewer places");
        assert_eq!(FixedPoint::parse("4", 3), Some(4_000), "a bare integer");
        for text in [
            "-0", "-0.00", "0.001", "1e1", ".5", "5.", "+1", "", "-", "1.2.3",
        ] {
            assert_eq!(FixedPoint::parse(text, 2), None, "{}", text);
        }
        assert_eq!(FixedPoint::parse("0.7071", 3), None, "never rounded");
    }

    #[test]
    fn a_filter_is_held_to_the_room_correction_bounds_on_every_field() {
        let ok = EqFilter {
            freq_hz: 120,
            gain_cdb: -350,
            q_milli: 4_000,
        };
        assert_eq!(ok.problem(), None);
        for (bad, field) in [
            (EqFilter { freq_hz: 19, ..ok }, "freq_hz"),
            (
                EqFilter {
                    freq_hz: 1001,
                    ..ok
                },
                "freq_hz",
            ),
            (
                EqFilter {
                    gain_cdb: 301,
                    ..ok
                },
                "gain_db",
            ),
            (
                EqFilter {
                    gain_cdb: -1201,
                    ..ok
                },
                "gain_db",
            ),
            (EqFilter { q_milli: 499, ..ok }, "q"),
            (
                EqFilter {
                    q_milli: 10_001,
                    ..ok
                },
                "q",
            ),
        ] {
            let p = bad.problem().expect("refused");
            assert!(p.starts_with(field), "{}: {}", field, p);
        }
        for edge in [
            EqFilter {
                freq_hz: 20,
                gain_cdb: -1200,
                q_milli: 500,
            },
            EqFilter {
                freq_hz: 1000,
                gain_cdb: 300,
                q_milli: 10_000,
            },
        ] {
            assert_eq!(edge.problem(), None, "{:?}", edge);
        }
    }

    #[test]
    fn a_filter_survives_its_persisted_spelling() {
        let text = "120 -3.50 4.000";
        assert_eq!(EqFilter::from_persisted(text).unwrap().persisted(), text);
        assert!(
            EqFilter::from_persisted("120 4.00 4.000").is_err(),
            "a boost"
        );
        assert!(EqFilter::from_persisted("0120 -3.50 4.000").is_err());
        assert!(EqFilter::from_persisted("120 -3.50").is_err());
    }
}
