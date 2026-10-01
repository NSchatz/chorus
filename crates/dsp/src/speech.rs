//! Speech enhancement: a voice-band boost where the dialogue is.
//!
//! With a centre channel the dialogue is mostly in it, so the boost is a
//! peaking section on FC. With stereo it is mostly in the middle of the
//! image, so the signal is split into mid `m = (l + r)/2` and side
//! `s = (l - r)/2`, the boost goes on `m`, and `l = m' + s`, `r = m' - s` put
//! it back (a side-only signal, `l = -r`, passes unchanged; the fixtures check
//! both). With mono the boost goes on the one channel. This is the "simple
//! center extraction and gain" that J. T. Geiger, P. Grosche and Y. Lacouture
//! Parodi use as their baseline, "in which the center is amplified (by 3.8 dB)
//! with respect to the left and right channels" ("Dialogue enhancement of
//! stereo sound", EUSIPCO 2015,
//! <https://www.eurasip.org/Proceedings/Eusipco/Eusipco2015/papers/1570096395.pdf>,
//! read 2026-10-01), so the gain is theirs rounded to +4 dB. The voice band is
//! "about 1-4 kHz" (`research-theater.md` section 5.3, a search snippet), so
//! the centre and width below are chorus's ASSUMED reading of it, not
//! measured.

use crate::biquad::{Coefficients, Kind};
use crate::DspError;

/// The boost's centre: 2 kHz, the geometric middle of 1-4 kHz. ASSUMED.
pub const CENTRE_HZ: f64 = 2000.0;
/// The boost's Q: 2/3, two octaves between the cookbook's dBgain/2 points
/// (1 to 4 kHz). ASSUMED.
pub const Q: f64 = 0.667;
/// The boost at the centre: +4 dB, Geiger et al.'s 3.8 dB centre gain
/// rounded (cited above).
pub const GAIN_DB: f64 = 4.0;

/// Where the boost goes, from the stream's channel map.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// On the channel at this index (FC, or the only channel of a mono
    /// stream).
    Channel(usize),
    /// On the mid of the FL and FR channels at these indexes.
    MidSide(usize, usize),
    /// Nowhere: the map has no centre, no front pair and more than one
    /// channel.
    None,
}

/// Chooses the mode for a channel map (positions as `docs/protocol.md`'s
/// channel map numbers them).
pub fn mode_for(map: &[u8]) -> Mode {
    use crate::settings::position::{FC, FL, FR};
    if let Some(c) = map.iter().position(|&p| p == FC) {
        return Mode::Channel(c);
    }
    let l = map.iter().position(|&p| p == FL);
    let r = map.iter().position(|&p| p == FR);
    if let (Some(l), Some(r)) = (l, r) {
        return Mode::MidSide(l, r);
    }
    if map.len() == 1 {
        return Mode::Channel(0);
    }
    Mode::None
}

/// The boost's section at `rate_hz` (its centre held below Nyquist the way
/// the chain holds every corner, `crate::chain::corner`).
pub fn design(rate_hz: f64) -> Result<Coefficients, DspError> {
    Coefficients::design(
        Kind::Peaking,
        rate_hz,
        crate::chain::corner(CENTRE_HZ, rate_hz),
        Q,
        GAIN_DB,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_mode_follows_the_map() {
        assert_eq!(mode_for(&[1, 2, 3, 4, 5, 6]), Mode::Channel(2));
        assert_eq!(mode_for(&[1, 2]), Mode::MidSide(0, 1));
        assert_eq!(mode_for(&[0]), Mode::Channel(0));
        assert_eq!(mode_for(&[5, 6]), Mode::None);
        let c = design(48000.0).unwrap();
        assert!((c.magnitude_db(CENTRE_HZ, 48000.0) - GAIN_DB).abs() < 1e-9);
    }
}
