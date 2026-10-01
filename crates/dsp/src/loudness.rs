//! Loudness compensation from the ISO 226:2003 equal-loudness-level contours.
//!
//! The ear loses the bass (and some of the top) faster than the midrange as
//! the level falls, so music turned down sounds thin. ISO 226:2003
//! ("Acoustics: Normal equal-loudness-level contours",
//! <https://www.iso.org/standard/34222.html>) gives the sound pressure level
//! `Lp` that sounds as loud as a 1 kHz tone of `Ln` phon, from a parameter
//! table (`af`, `Lu`, `Tf` at 29 frequencies) and a formula:
//!
//! ```text
//! Af = 4.47e-3 (10^(0.025 Ln) - 1.15) + (0.4 * 10^((Tf + Lu)/10 - 9))^af
//! Lp = (10 / af) log10(Af) - Lu + 94
//! ```
//!
//! The standard itself is sold, not published; the table and the formula
//! here are as reproduced at <https://www.dsprelated.com/showcode/174.php>
//! (read 2026-10-01), and `fixtures/dsp/iso226-*.txt` carries the rows it
//! checks, so a typo in the table below fails a test against the reproduced
//! values rather than passing silently. The formula is valid from 20 phon
//! (and to 90 phon below 5 kHz, 80 above).
//!
//! What chorus does with it (chorus's own design, every constant ASSUMED and
//! named below): the playback is `att` dB below the reference level. At a
//! listening level of `L = REFERENCE_PHON - att` phon (held to 20..80), the
//! boost a frequency needs to keep its balance with 1 kHz is
//! `(Lp(f, L) - Lp(1k, L)) - (Lp(f, ref) - Lp(1k, ref))`. That is taken at
//! [`LOW_EVAL_HZ`] for a low shelf at [`LOW_SHELF_HZ`] and at
//! [`HIGH_EVAL_HZ`] for a high shelf at [`HIGH_SHELF_HZ`], each held to
//! `0..=cap`. At unity volume `att` is 0 and both gains are exactly 0, so the
//! stage is bypassed.

/// The table's frequencies, Hz.
pub const FREQS_HZ: [f64; 29] = [
    20.0, 25.0, 31.5, 40.0, 50.0, 63.0, 80.0, 100.0, 125.0, 160.0, 200.0, 250.0, 315.0, 400.0,
    500.0, 630.0, 800.0, 1000.0, 1250.0, 1600.0, 2000.0, 2500.0, 3150.0, 4000.0, 5000.0, 6300.0,
    8000.0, 10000.0, 12500.0,
];

/// The exponent for loudness perception, `af`.
pub const AF: [f64; 29] = [
    0.532, 0.506, 0.480, 0.455, 0.432, 0.409, 0.387, 0.367, 0.349, 0.330, 0.315, 0.301, 0.288,
    0.276, 0.267, 0.259, 0.253, 0.250, 0.246, 0.244, 0.243, 0.243, 0.243, 0.242, 0.242, 0.245,
    0.254, 0.271, 0.301,
];

/// The magnitude of the linear transfer function normalised at 1 kHz, `Lu`, dB.
pub const LU: [f64; 29] = [
    -31.6, -27.2, -23.0, -19.1, -15.9, -13.0, -10.3, -8.1, -6.2, -4.5, -3.1, -2.0, -1.1, -0.4, 0.0,
    0.3, 0.5, 0.0, -2.7, -4.1, -1.0, 1.7, 2.5, 1.2, -2.1, -7.1, -11.2, -10.7, -3.1,
];

/// The threshold of hearing, `Tf`, dB SPL.
pub const TF: [f64; 29] = [
    78.5, 68.7, 59.5, 51.1, 44.0, 37.5, 31.5, 26.5, 22.1, 17.9, 14.4, 11.4, 8.6, 6.2, 4.4, 3.0,
    2.2, 2.4, 3.5, 1.7, -1.3, -4.2, -6.0, -5.4, -1.5, 6.0, 12.6, 13.9, 12.3,
];

/// The index of `freq_hz` in [`FREQS_HZ`], if it is one of the table's.
pub fn table_index(freq_hz: f64) -> Option<usize> {
    FREQS_HZ.iter().position(|&f| f == freq_hz)
}

/// `Lp`, the SPL (dB) of a tone at table frequency `freq_hz` with loudness
/// level `phon`, by the ISO 226:2003 formula. `None` for a frequency not in
/// the table.
pub fn spl_db(freq_hz: f64, phon: f64) -> Option<f64> {
    let i = table_index(freq_hz)?;
    let af = 4.47e-3 * (10f64.powf(0.025 * phon) - 1.15)
        + (0.4 * 10f64.powf((TF[i] + LU[i]) / 10.0 - 9.0)).powf(AF[i]);
    Some((10.0 / AF[i]) * af.log10() - LU[i] + 94.0)
}

/// The level the volume is measured down from: unity volume plays at this
/// loudness level. ASSUMED: 80 phon, the top of the formula's range above
/// 5 kHz; not measured against a real speaker's SPL.
pub const REFERENCE_PHON: f64 = 80.0;
/// The lowest listening level the formula is used at (its validity floor).
pub const MIN_PHON: f64 = 20.0;
/// Where the low compensation is evaluated. ASSUMED: 50 Hz.
pub const LOW_EVAL_HZ: f64 = 50.0;
/// Where the high compensation is evaluated. ASSUMED: 10 kHz.
pub const HIGH_EVAL_HZ: f64 = 10000.0;
/// The low shelf's midpoint. ASSUMED: 100 Hz.
pub const LOW_SHELF_HZ: f64 = 100.0;
/// The high shelf's midpoint. ASSUMED: 8 kHz.
pub const HIGH_SHELF_HZ: f64 = 8000.0;
/// Both shelves' Q (the cookbook's S = 1 slope for small gains). ASSUMED.
pub const SHELF_Q: f64 = core::f64::consts::FRAC_1_SQRT_2;
/// The low shelf's cap. ASSUMED: +12 dB.
pub const LOW_CAP_DB: f64 = 12.0;
/// The high shelf's cap. ASSUMED: +6 dB.
pub const HIGH_CAP_DB: f64 = 6.0;
/// The attenuation is taken in steps of this many dB, so a volume ramp
/// redesigns the shelves at most once per step. ASSUMED: 0.5 dB.
pub const STEP_DB: f64 = 0.5;

/// The boost (dB) a frequency needs `att_db` below the reference, before the
/// cap: `(Lp(f, L) - Lp(1k, L)) - (Lp(f, ref) - Lp(1k, ref))`.
pub fn needed_db(freq_hz: f64, att_db: f64) -> Option<f64> {
    let level = (REFERENCE_PHON - att_db).clamp(MIN_PHON, REFERENCE_PHON);
    let at = spl_db(freq_hz, level)? - spl_db(1000.0, level)?;
    let reference = spl_db(freq_hz, REFERENCE_PHON)? - spl_db(1000.0, REFERENCE_PHON)?;
    Some(at - reference)
}

/// The attenuation below the reference, quantised to [`STEP_DB`], from the
/// linear room gain the chain is handed (0 is held at the bottom of the
/// range).
pub fn attenuation_db(room_gain: f32) -> f64 {
    let g = room_gain as f64;
    let att = if g > 0.0 {
        -20.0 * g.log10()
    } else {
        REFERENCE_PHON
    };
    let att = att.clamp(0.0, REFERENCE_PHON - MIN_PHON);
    (att / STEP_DB + 0.5).floor() * STEP_DB
}

/// The low and high shelf gains (dB) for `att_db` below the reference.
pub fn shelf_gains_db(att_db: f64) -> (f64, f64) {
    if att_db <= 0.0 {
        return (0.0, 0.0);
    }
    let low = needed_db(LOW_EVAL_HZ, att_db).unwrap_or(0.0);
    let high = needed_db(HIGH_EVAL_HZ, att_db).unwrap_or(0.0);
    (low.clamp(0.0, LOW_CAP_DB), high.clamp(0.0, HIGH_CAP_DB))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_thousand_hertz_is_close_to_its_own_phon() {
        // The phon is defined at 1 kHz; the formula holds it to about 0.1 dB.
        for phon in [20.0, 40.0, 60.0, 80.0] {
            let spl = spl_db(1000.0, phon).unwrap();
            assert!((spl - phon).abs() < 0.2, "{phon}: {spl}");
        }
    }

    #[test]
    fn quieter_means_more_bass_and_unity_means_none() {
        assert_eq!(shelf_gains_db(0.0), (0.0, 0.0));
        let mut last = 0.0;
        for att in [10.0, 20.0, 30.0, 40.0] {
            let (low, _) = shelf_gains_db(att);
            assert!(low >= last, "{att}: {low}");
            last = low;
        }
        assert!(last > 0.0);
        assert_eq!(attenuation_db(1.0), 0.0);
        assert_eq!(attenuation_db(0.5), 6.0);
        assert_eq!(attenuation_db(0.0), 60.0);
    }
}
