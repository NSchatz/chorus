//! Channel count conversion for a decode on its way to a stream.

/// Appends `input` (interleaved, `from_channels` wide) to `out` as
/// `to_channels` wide frames.
///
/// - The same count: a copy.
/// - Mono to stereo or more: the one channel on both front channels, at
///   unchanged level; any further channels are silent.
/// - Stereo to mono: the mean of the two.
/// - Stereo to more: the front pair; the further channels are silent.
/// - More than two to stereo: a downmix. Channels are taken in the order WAVE
///   and FLAC carry them (front left, front right, front centre, LFE, then the
///   surround pairs; `ASSUMED` to be the order Symphonia hands every codec's
///   channels out in). The centre and the surrounds go into their side at
///   1/sqrt(2) and the LFE is left out, the coefficients of ITU-R BS.775
///   (`ASSUMED`: quoted from memory of the recommendation's downmix table, not
///   re-read), and each output is scaled by one over the sum of its
///   coefficients so that full-scale input cannot clip.
/// - More than two to mono, or to another count above two: through stereo.
///
/// A trailing partial frame is ignored. Zero channels on either side appends
/// nothing.
pub fn remix(input: &[f32], from_channels: u16, to_channels: u16, out: &mut Vec<f32>) {
    let from = usize::from(from_channels);
    let to = usize::from(to_channels);
    if from == 0 || to == 0 {
        return;
    }
    let frames = input.len() / from;
    out.reserve(frames * to);
    if from == to {
        out.extend_from_slice(&input[..frames * from]);
        return;
    }
    let weights = if from > 2 {
        stereo_weights(from)
    } else {
        Vec::new()
    };
    for frame in input.chunks_exact(from) {
        let (left, right) = match from {
            1 => (frame[0], frame[0]),
            2 => (frame[0], frame[1]),
            _ => {
                let mut l = 0.0f32;
                let mut r = 0.0f32;
                for (s, (wl, wr)) in frame.iter().zip(&weights) {
                    l += s * wl;
                    r += s * wr;
                }
                (l, r)
            }
        };
        match to {
            1 => out.push(0.5 * (left + right)),
            _ => {
                out.push(left);
                out.push(right);
                out.extend(std::iter::repeat_n(0.0, to - 2));
            }
        }
    }
}

/// Per input channel, its weight into the left and the right output.
fn stereo_weights(channels: usize) -> Vec<(f32, f32)> {
    const S: f32 = std::f32::consts::FRAC_1_SQRT_2;
    // Which side(s) each channel of the WAVE order feeds: L, R, both (centre), none (LFE).
    #[derive(Clone, Copy)]
    enum To {
        L,
        R,
        Both,
        None,
    }
    use To::{Both, None, L, R};
    let layout: &[To] = match channels {
        3 => &[L, R, Both],
        4 => &[L, R, L, R],
        5 => &[L, R, Both, L, R],
        6 => &[L, R, Both, None, L, R],
        7 => &[L, R, Both, None, Both, L, R],
        8 => &[L, R, Both, None, L, R, L, R],
        // An unknown layout: the front pair alone.
        _ => &[L, R],
    };
    let mut weights = vec![(0.0f32, 0.0f32); channels];
    for (i, to) in layout.iter().enumerate() {
        let side = if i < 2 { 1.0 } else { S };
        weights[i] = match to {
            L => (side, 0.0),
            R => (0.0, side),
            Both => (S, S),
            None => (0.0, 0.0),
        };
    }
    let sum_l: f32 = weights.iter().map(|w| w.0).sum();
    let sum_r: f32 = weights.iter().map(|w| w.1).sum();
    for w in &mut weights {
        w.0 /= sum_l;
        w.1 /= sum_r;
    }
    weights
}

#[cfg(test)]
mod tests {
    use super::remix;

    #[test]
    fn mono_goes_to_both_front_channels_and_back() {
        let mut out = Vec::new();
        remix(&[0.25, -0.5], 1, 2, &mut out);
        assert_eq!(out, [0.25, 0.25, -0.5, -0.5]);
        let mut mono = Vec::new();
        remix(&out, 2, 1, &mut mono);
        assert_eq!(mono, [0.25, -0.5]);
    }

    #[test]
    fn the_same_count_is_a_copy_and_a_partial_frame_is_dropped() {
        let mut out = vec![9.0];
        remix(&[0.1, 0.2, 0.3], 2, 2, &mut out);
        assert_eq!(out, [9.0, 0.1, 0.2]);
    }

    #[test]
    fn stereo_to_six_is_the_front_pair() {
        let mut out = Vec::new();
        remix(&[0.5, -0.5], 2, 6, &mut out);
        assert_eq!(out, [0.5, -0.5, 0.0, 0.0, 0.0, 0.0]);
    }

    #[test]
    fn a_five_one_downmix_keeps_full_scale_in_range_and_drops_the_lfe() {
        let mut out = Vec::new();
        // Every channel at full scale: the scaled sum is exactly full scale.
        remix(&[1.0; 6], 6, 2, &mut out);
        assert!(
            (out[0] - 1.0).abs() < 1e-6 && (out[1] - 1.0).abs() < 1e-6,
            "{out:?}"
        );
        // The LFE alone is silent; the centre alone is on both sides equally.
        out.clear();
        remix(&[0.0, 0.0, 0.0, 1.0, 0.0, 0.0], 6, 2, &mut out);
        assert_eq!(out, [0.0, 0.0]);
        out.clear();
        remix(&[0.0, 0.0, 1.0, 0.0, 0.0, 0.0], 6, 2, &mut out);
        assert!(out[0] > 0.2 && out[0] == out[1], "{out:?}");
        // To mono: through stereo.
        out.clear();
        remix(&[1.0; 6], 6, 1, &mut out);
        assert!((out[0] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn zero_channels_appends_nothing() {
        let mut out = Vec::new();
        remix(&[1.0, 1.0], 0, 2, &mut out);
        remix(&[1.0, 1.0], 2, 0, &mut out);
        assert!(out.is_empty());
    }
}
