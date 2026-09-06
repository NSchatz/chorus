//! Zone volume and mute, applied to the PCM on its way to the sink.
//!
//! # Where this runs, and why it is here rather than anywhere else
//!
//! At the very last point before [`crate::sink::PcmSink::write`]. Everything
//! upstream of it - the buffer, the sync loop, the playout corrector - is about
//! WHEN a sample becomes audible, and this is the only thing in the client that
//! changes WHAT the sample is. Putting it last means the sync loop's frame
//! accounting is untouched by it: a muted zone writes exactly as many frames as
//! an unmuted one, at exactly the same instants, and the device's reported
//! delay is what it would have been. A mute that stopped writing would be a
//! mute that silently changed the endpoint's alignment, and coming back from it
//! would be a resync.
//!
//! # It is graded on the samples, never on a status line
//!
//! `crates/client-linux/tests/zone_apply.rs` reads what a modelled
//! [`crate::sink::PcmSink`] ACCEPTED and asserts the scaling on those bytes.
//! That is the criterion's own wording, and it is why this is a pure function
//! over a buffer with no reporting in it at all: there is nothing here that
//! could say it had applied a volume it had not.
//!
//! # The factor is exact, and the error is one unit in the last place
//!
//! The catalog carries a volume as thousandths of full scale
//! (`docs/decisions/0016-the-control-catalog.md`), so scaling an integer sample
//! is an integer multiply and an integer divide, with no floating point on the
//! path at all for the two integer formats. Truncation toward zero is the one
//! rounding this does, so a scaled sample is within one unit in the last place
//! of the exact product, in the direction of silence. `pcm_f32le` is scaled in
//! `f32` because its samples already are.

use chorus_control::catalog::{Volume, VOLUME_SCALE};
use chorus_protocol::SampleFormat;

/// How far a scaled sample may be from the exact product, in units of the last
/// place.
///
/// One, and in the direction of silence, because the only rounding here is the
/// truncation of an integer divide. The test asserts this bound rather than
/// asserting equality, and this constant is what it asserts.
pub const SCALING_TOLERANCE_ULP: i64 = 1;

/// Applies a zone's gain to interleaved PCM in the format the stream announced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ZoneGain {
    format: SampleFormat,
}

impl ZoneGain {
    /// A gain applier for one stream's format.
    pub fn new(format: SampleFormat) -> ZoneGain {
        ZoneGain { format }
    }

    /// Scale `pcm` in place by `gain`.
    ///
    /// Full scale is the identity and returns without touching a byte, which is
    /// the ordinary case and the one that has to cost nothing. Silence writes
    /// zeros rather than multiplying by zero, which is the same answer and is
    /// exact for every format including the float one.
    pub fn apply(&self, gain: Volume, pcm: &mut [u8]) {
        if gain == Volume::FULL {
            return;
        }
        if gain == Volume::SILENT {
            // A muted zone hands the device the same number of frames it would
            // otherwise have, and every sample in them is zero. Frames keep
            // flowing; only their content changes.
            pcm.fill(0);
            return;
        }
        let numerator = i64::from(gain.thousandths());
        let denominator = i64::from(VOLUME_SCALE);
        match self.format {
            SampleFormat::PcmS16Le => {
                for sample in pcm.chunks_exact_mut(2) {
                    let value = i64::from(i16::from_le_bytes([sample[0], sample[1]]));
                    let scaled = (value * numerator / denominator) as i16;
                    sample.copy_from_slice(&scaled.to_le_bytes());
                }
            }
            SampleFormat::PcmS24Le => {
                for sample in pcm.chunks_exact_mut(3) {
                    let value = sign_extend_24(sample);
                    let scaled = value * numerator / denominator;
                    let bytes = (scaled as i32).to_le_bytes();
                    sample.copy_from_slice(&bytes[..3]);
                }
            }
            SampleFormat::PcmF32Le => {
                let factor = gain.thousandths() as f32 / VOLUME_SCALE as f32;
                for sample in pcm.chunks_exact_mut(4) {
                    let value =
                        f32::from_le_bytes([sample[0], sample[1], sample[2], sample[3]]);
                    sample.copy_from_slice(&(value * factor).to_le_bytes());
                }
            }
        }
    }
}

/// A packed 24-bit little-endian sample as the number it is.
fn sign_extend_24(bytes: &[u8]) -> i64 {
    let raw = u32::from(bytes[0]) | (u32::from(bytes[1]) << 8) | (u32::from(bytes[2]) << 16);
    if raw & 0x0080_0000 != 0 {
        i64::from(raw as i32 | !0x00FF_FFFF)
    } else {
        i64::from(raw as i32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s16(samples: &[i16]) -> Vec<u8> {
        samples.iter().flat_map(|s| s.to_le_bytes()).collect()
    }

    fn read_s16(pcm: &[u8]) -> Vec<i16> {
        pcm.chunks_exact(2)
            .map(|c| i16::from_le_bytes([c[0], c[1]]))
            .collect()
    }

    #[test]
    fn full_scale_changes_nothing_at_all() {
        let original = s16(&[-32_768, -1, 0, 1, 32_767]);
        let mut pcm = original.clone();
        ZoneGain::new(SampleFormat::PcmS16Le).apply(Volume::FULL, &mut pcm);
        assert_eq!(pcm, original);
    }

    #[test]
    fn silence_is_every_sample_zero_and_the_same_number_of_frames() {
        let mut pcm = s16(&[-32_768, -1, 3_000, 32_767]);
        let frames = pcm.len();
        ZoneGain::new(SampleFormat::PcmS16Le).apply(Volume::SILENT, &mut pcm);
        assert_eq!(pcm.len(), frames, "a mute must not shorten a chunk");
        assert!(pcm.iter().all(|b| *b == 0));
    }

    #[test]
    fn a_scaled_sample_is_within_one_unit_of_the_exact_product() {
        let gain = Volume::from_thousandths(375).unwrap();
        let samples: Vec<i16> = (-32_768..32_767).step_by(7).collect();
        let mut pcm = s16(&samples);
        ZoneGain::new(SampleFormat::PcmS16Le).apply(gain, &mut pcm);
        for (before, after) in samples.iter().zip(read_s16(&pcm)) {
            let exact = f64::from(*before) * 0.375;
            let error = f64::from(after) - exact;
            assert!(
                error.abs() <= SCALING_TOLERANCE_ULP as f64,
                "{} scaled to {} against an exact {}",
                before,
                after,
                exact
            );
        }
    }

    #[test]
    fn the_twenty_four_bit_format_keeps_its_sign() {
        let gain = Volume::from_thousandths(500).unwrap();
        // -8388608, -1, 1, 8388607, packed little-endian in three bytes each.
        let mut pcm = vec![0x00, 0x00, 0x80, 0xFF, 0xFF, 0xFF, 0x01, 0x00, 0x00, 0xFF, 0xFF, 0x7F];
        ZoneGain::new(SampleFormat::PcmS24Le).apply(gain, &mut pcm);
        let values: Vec<i64> = pcm.chunks_exact(3).map(sign_extend_24).collect();
        assert_eq!(values, vec![-4_194_304, 0, 0, 4_194_303]);
    }

    #[test]
    fn the_float_format_is_scaled_as_a_float() {
        let gain = Volume::from_thousandths(250).unwrap();
        let mut pcm: Vec<u8> = [-1.0f32, -0.5, 0.0, 0.5, 1.0]
            .iter()
            .flat_map(|s| s.to_le_bytes())
            .collect();
        ZoneGain::new(SampleFormat::PcmF32Le).apply(gain, &mut pcm);
        let values: Vec<f32> = pcm
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect();
        assert_eq!(values, vec![-0.25, -0.125, 0.0, 0.125, 0.25]);
    }

    #[test]
    fn scaling_never_wraps_at_either_extreme() {
        // The one arithmetic hazard here: a multiply that overflowed would turn
        // the loudest possible sample into the quietest, which is a click at
        // full scale and would be inaudible in a test that only checked the
        // middle of the range.
        for thousandths in [0i64, 1, 499, 500, 501, 999, 1_000] {
            let gain = Volume::from_thousandths(thousandths).unwrap();
            let mut pcm = s16(&[i16::MIN, i16::MAX]);
            ZoneGain::new(SampleFormat::PcmS16Le).apply(gain, &mut pcm);
            let values = read_s16(&pcm);
            assert!(values[0] <= 0, "{} turned i16::MIN into {}", thousandths, values[0]);
            assert!(values[1] >= 0, "{} turned i16::MAX into {}", thousandths, values[1]);
        }
    }
}
