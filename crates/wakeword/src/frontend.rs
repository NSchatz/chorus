// This file is a Rust port of TensorFlow Lite's micro frontend
// (tensorflow/lite/experimental/microfrontend/lib: window, fft, filterbank,
// noise_reduction, pcan_gain_control, log_scale, log_lut and frontend), which
// is:
//
// Copyright 2018 The TensorFlow Authors. All Rights Reserved.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// Changes from the original: translated from C to Rust; the configuration is
// fixed to the one microWakeWord trains with; buffers are owned vectors.

//! The audio frontend: 16 kHz mono PCM in, one 40-channel feature frame out
//! per 10 ms.
//!
//! microWakeWord models are trained on the output of TensorFlow Lite's micro
//! frontend with one fixed configuration, so the detector has to compute the
//! same numbers: a 30 ms Hann window every 10 ms, a 512-point fixed-point FFT,
//! a 40-channel mel filterbank from 125 Hz to 7500 Hz, noise reduction,
//! per-channel automatic gain (PCAN) and a log scale. All of it is integer
//! arithmetic on tables built once; this port keeps the original's order of
//! operations, widths and rounding, and `tests/reference.rs` holds it to the
//! original's output on the committed fixtures, bit for bit.

use crate::fft::{Complex, RealFft, FFT_SIZE};

/// The sample rate the frontend is built for.
pub const SAMPLE_RATE: u32 = 16_000;
/// Samples between one feature frame and the next (10 ms).
pub const STEP: usize = 160;
/// Samples in one analysis window (30 ms).
pub const WINDOW: usize = 480;
/// Mel channels in one feature frame.
pub const CHANNELS: usize = 40;

const WINDOW_BITS: u32 = 12;
const FILTERBANK_BITS: u32 = 12;
const LOWER_BAND_LIMIT: f32 = 125.0;
const UPPER_BAND_LIMIT: f32 = 7500.0;
const SPECTRUM: usize = FFT_SIZE / 2 + 1;
/// The filterbank reads its channels in blocks of four bins from an even start.
const CHANNEL_BLOCK: usize = 4;
const INDEX_ALIGNMENT: usize = 2;

const NOISE_REDUCTION_BITS: u32 = 14;
const SMOOTHING_BITS: u32 = 10;
const EVEN_SMOOTHING: f32 = 0.025;
const ODD_SMOOTHING: f32 = 0.06;
const MIN_SIGNAL_REMAINING: f32 = 0.05;

const PCAN_SNR_BITS: u32 = 12;
const PCAN_OUTPUT_BITS: u32 = 6;
const PCAN_STRENGTH: f32 = 0.95;
const PCAN_OFFSET: f32 = 80.0;
const PCAN_GAIN_BITS: u32 = 21;
const WIDE_DYNAMIC_BITS: u32 = 32;
const GAIN_LUT_SIZE: usize = 4 * WIDE_DYNAMIC_BITS as usize - 3;

const LOG_SCALE_SHIFT: u32 = 6;
const LOG_SEGMENTS_LOG2: u32 = 7;
const LOG_SCALE_LOG2: u32 = 16;
const LOG_SCALE: u32 = 1 << LOG_SCALE_LOG2;
const LOG_COEFF: u64 = 45426;

/// What the gain and the log scale shift their input by: `log2(512) - 1 - FILTERBANK_BITS / 2`.
const CORRECTION_BITS: u32 = 3;

#[rustfmt::skip]
const LOG_LUT: [u16; 130] = [
    0,    224,  442,  654,  861,  1063, 1259, 1450, 1636, 1817, 1992, 2163,
    2329, 2490, 2646, 2797, 2944, 3087, 3224, 3358, 3487, 3611, 3732, 3848,
    3960, 4068, 4172, 4272, 4368, 4460, 4549, 4633, 4714, 4791, 4864, 4934,
    5001, 5063, 5123, 5178, 5231, 5280, 5326, 5368, 5408, 5444, 5477, 5507,
    5533, 5557, 5578, 5595, 5610, 5622, 5631, 5637, 5640, 5641, 5638, 5633,
    5626, 5615, 5602, 5586, 5568, 5547, 5524, 5498, 5470, 5439, 5406, 5370,
    5332, 5291, 5249, 5203, 5156, 5106, 5054, 5000, 4944, 4885, 4825, 4762,
    4697, 4630, 4561, 4490, 4416, 4341, 4264, 4184, 4103, 4020, 3935, 3848,
    3759, 3668, 3575, 3481, 3384, 3286, 3186, 3084, 2981, 2875, 2768, 2659,
    2549, 2437, 2323, 2207, 2090, 1971, 1851, 1729, 1605, 1480, 1353, 1224,
    1094, 963,  830,  695,  559,  421,  282,  142,  0,    0,
];

/// The position of the highest set bit, counted from 1; 0 for 0.
#[inline]
fn msb32(n: u32) -> u32 {
    32 - n.leading_zeros()
}

fn freq_to_mel(freq: f32) -> f32 {
    1127.0 * (freq / 700.0).ln_1p()
}

/// The triangular mel filters as the original lays them out: per channel a start bin, a
/// start in the weight arrays and a width, with widths padded to blocks of four.
struct Filterbank {
    start_index: usize,
    end_index: usize,
    frequency_starts: Vec<usize>,
    weight_starts: Vec<usize>,
    widths: Vec<usize>,
    weights: Vec<i16>,
    unweights: Vec<i16>,
}

impl Filterbank {
    fn new() -> Self {
        let channels = CHANNELS + 1;
        let mel_low = freq_to_mel(LOWER_BAND_LIMIT);
        let mel_spacing = (freq_to_mel(UPPER_BAND_LIMIT) - mel_low) / channels as f32;
        let centers: Vec<f32> = (0..channels)
            .map(|i| mel_low + mel_spacing * (i + 1) as f32)
            .collect();

        // Always exclude DC.
        let hz_per_bin = 0.5 * SAMPLE_RATE as f32 / (SPECTRUM as f32 - 1.0);
        let start_index = (1.5 + LOWER_BAND_LIMIT / hz_per_bin) as usize;

        let mut frequency_starts = vec![0usize; channels];
        let mut weight_starts = vec![0usize; channels];
        let mut widths = vec![0usize; channels];
        let mut actual_starts = vec![0usize; channels];
        let mut actual_widths = vec![0usize; channels];
        let mut chan_freq_index_start = start_index;
        let mut weight_index_start = 0usize;
        let mut needs_zeros = false;
        for chan in 0..channels {
            // Keep taking bins until one passes this channel's centre.
            let mut freq_index = chan_freq_index_start;
            while freq_to_mel(freq_index as f32 * hz_per_bin) <= centers[chan] {
                freq_index += 1;
            }
            let width = freq_index - chan_freq_index_start;
            actual_starts[chan] = chan_freq_index_start;
            actual_widths[chan] = width;
            if width == 0 {
                // A channel no bin falls into multiplies one block of zero weights, which
                // sit at the front of the weight arrays.
                frequency_starts[chan] = 0;
                weight_starts[chan] = 0;
                widths[chan] = CHANNEL_BLOCK;
                if !needs_zeros {
                    needs_zeros = true;
                    for start in weight_starts.iter_mut().take(chan) {
                        *start += CHANNEL_BLOCK;
                    }
                    weight_index_start += CHANNEL_BLOCK;
                }
            } else {
                let aligned_start = (chan_freq_index_start / INDEX_ALIGNMENT) * INDEX_ALIGNMENT;
                let aligned_width = chan_freq_index_start - aligned_start + width;
                let padded_width = ((aligned_width - 1) / CHANNEL_BLOCK + 1) * CHANNEL_BLOCK;
                frequency_starts[chan] = aligned_start;
                weight_starts[chan] = weight_index_start;
                widths[chan] = padded_width;
                weight_index_start += padded_width;
            }
            chan_freq_index_start = freq_index;
        }

        let mut weights = vec![0i16; weight_index_start];
        let mut unweights = vec![0i16; weight_index_start];
        let mut end_index = 0;
        let one = (1u32 << FILTERBANK_BITS) as f32;
        for chan in 0..channels {
            let mut frequency = actual_starts[chan];
            let frequency_offset = frequency - frequency_starts[chan];
            let denom = if chan == 0 {
                mel_low
            } else {
                centers[chan - 1]
            };
            for j in 0..actual_widths[chan] {
                let weight = (centers[chan] - freq_to_mel(frequency as f32 * hz_per_bin))
                    / (centers[chan] - denom);
                let at = weight_starts[chan] + frequency_offset + j;
                weights[at] = (weight * one + 0.5).floor() as i16;
                unweights[at] = ((1.0 - weight) * one + 0.5).floor() as i16;
                frequency += 1;
            }
            end_index = end_index.max(frequency);
        }
        Self {
            start_index,
            end_index,
            frequency_starts,
            weight_starts,
            widths,
            weights,
            unweights,
        }
    }
}

fn sqrt32(mut num: u32) -> u32 {
    if num == 0 {
        return 0;
    }
    let mut res: u32 = 0;
    let max_bit_number = (32 - msb32(num)) | 1;
    let mut bit = 1u32 << (31 - max_bit_number);
    for _ in 0..(31 - max_bit_number) / 2 + 1 {
        if num >= res + bit {
            num -= res + bit;
            res = (res >> 1) + bit;
        } else {
            res >>= 1;
        }
        bit >>= 2;
    }
    // Round, if there is room.
    if num > res && res != 0xFFFF {
        res += 1;
    }
    // The original returns 16 bits.
    res & 0xFFFF
}

fn sqrt64(mut num: u64) -> u32 {
    // The original takes the 32-bit path whenever the upper word is clear.
    if num >> 32 == 0 {
        return sqrt32(num as u32);
    }
    let mut res: u64 = 0;
    let max_bit_number = (num.leading_zeros()) | 1;
    let mut bit = 1u64 << (63 - max_bit_number);
    for _ in 0..(63 - max_bit_number) / 2 + 1 {
        if num >= res + bit {
            num -= res + bit;
            res = (res >> 1) + bit;
        } else {
            res >>= 1;
        }
        bit >>= 2;
    }
    if num > res && res != 0xFFFF_FFFF {
        res += 1;
    }
    res as u32
}

/// The automatic gain's lookup: `2^gain_bits * (x / 2^input_bits + offset)^-strength`.
fn pcan_gain(input_bits: u32, x: u32) -> i16 {
    let x_as_float = x as f32 / (1u32 << input_bits) as f32;
    let gain = (1u32 << PCAN_GAIN_BITS) as f32 * (x_as_float + PCAN_OFFSET).powf(-PCAN_STRENGTH);
    if gain > 32767.0 {
        32767
    } else {
        (gain + 0.5) as i16
    }
}

fn gain_lut() -> [i16; GAIN_LUT_SIZE] {
    let input_bits = SMOOTHING_BITS - CORRECTION_BITS;
    let mut lut = [0i16; GAIN_LUT_SIZE];
    lut[0] = pcan_gain(input_bits, 0);
    lut[1] = pcan_gain(input_bits, 1);
    for interval in 2..=WIDE_DYNAMIC_BITS {
        let x0 = 1u32 << (interval - 1);
        let x1 = x0 + (x0 >> 1);
        let x2 = if interval == WIDE_DYNAMIC_BITS {
            x0 + (x0 - 1)
        } else {
            2 * x0
        };
        let y0 = pcan_gain(input_bits, x0);
        let diff1 = i32::from(pcan_gain(input_bits, x1)) - i32::from(y0);
        let diff2 = i32::from(pcan_gain(input_bits, x2)) - i32::from(y0);
        let a1 = 4 * diff1 - diff2;
        let a2 = diff2 - a1;
        let at = 4 * interval as usize - 6;
        lut[at] = y0;
        lut[at + 1] = a1 as i16;
        lut[at + 2] = a2 as i16;
    }
    lut
}

/// A piecewise quadratic through the gain table, over the whole 32-bit range of `x`.
fn wide_dynamic_function(x: u32, lut: &[i16; GAIN_LUT_SIZE]) -> i16 {
    if x <= 2 {
        return lut[x as usize];
    }
    let interval = msb32(x);
    let at = 4 * interval as usize - 6;
    let shifted = if interval < 11 {
        x << (11 - interval)
    } else {
        x >> (interval - 11)
    };
    let frac = (shifted & 0x3FF) as i32;
    let mut result = (i32::from(lut[at + 2]) * frac) >> 5;
    result = result.wrapping_add((i32::from(lut[at + 1]) as u32).wrapping_shl(5) as i32);
    result = result.wrapping_mul(frac);
    result = result.wrapping_add(1 << 14) >> 15;
    result = result.wrapping_add(i32::from(lut[at]));
    result as i16
}

fn pcan_shrink(x: u32) -> u32 {
    if x < (2 << PCAN_SNR_BITS) {
        (x * x) >> (2 + 2 * PCAN_SNR_BITS - PCAN_OUTPUT_BITS)
    } else {
        (x >> (PCAN_SNR_BITS - PCAN_OUTPUT_BITS)) - (1 << PCAN_OUTPUT_BITS)
    }
}

fn log2_fraction_part(x: u32, log2x: u32) -> u32 {
    let mut frac = (i64::from(x) - (1i64 << log2x)) as i32;
    if log2x < LOG_SCALE_LOG2 {
        frac <<= LOG_SCALE_LOG2 - log2x;
    } else {
        frac >>= log2x - LOG_SCALE_LOG2;
    }
    let base_seg = (frac as u32) >> (LOG_SCALE_LOG2 - LOG_SEGMENTS_LOG2);
    let seg_unit = (1u32 << LOG_SCALE_LOG2) >> LOG_SEGMENTS_LOG2;
    let c0 = i32::from(LOG_LUT[base_seg as usize]);
    let c1 = i32::from(LOG_LUT[base_seg as usize + 1]);
    let seg_base = (seg_unit * base_seg) as i32;
    let rel_pos = ((c1 - c0) * (frac - seg_base)) >> LOG_SCALE_LOG2;
    (frac + c0 + rel_pos) as u32
}

/// The natural logarithm of `x`, scaled by `2^LOG_SCALE_SHIFT`.
fn log(x: u32) -> u32 {
    let integer = msb32(x) - 1;
    let fraction = log2_fraction_part(x, integer);
    let log2 = (integer << LOG_SCALE_LOG2).wrapping_add(fraction);
    let round = LOG_SCALE / 2;
    let loge = ((LOG_COEFF * u64::from(log2) + u64::from(round)) >> LOG_SCALE_LOG2) as u32;
    ((loge << LOG_SCALE_SHIFT).wrapping_add(round)) >> LOG_SCALE_LOG2
}

/// The streaming frontend: feed it samples, get a feature frame whenever a
/// window is complete.
pub struct Frontend {
    coefficients: Vec<i16>,
    input: Vec<i16>,
    input_used: usize,
    fft: RealFft,
    fft_input: Box<[i16; FFT_SIZE]>,
    fft_output: Box<[Complex; SPECTRUM]>,
    filterbank: Filterbank,
    /// Squared magnitudes per bin. Longer than the spectrum because the filterbank's padded
    /// blocks may read past it; what they read there meets a zero weight.
    energy: Vec<u32>,
    work: [u64; CHANNELS + 1],
    estimate: [u32; CHANNELS],
    even_smoothing: u32,
    odd_smoothing: u32,
    min_signal_remaining: u32,
    gain_lut: [i16; GAIN_LUT_SIZE],
}

impl Default for Frontend {
    fn default() -> Self {
        Self::new()
    }
}

impl Frontend {
    /// A frontend with empty history.
    pub fn new() -> Self {
        let arg = std::f32::consts::PI * 2.0 / WINDOW as f32;
        let coefficients = (0..WINDOW)
            .map(|i| {
                let value = 0.5 - 0.5 * (arg * (i as f32 + 0.5)).cos();
                (value * (1u32 << WINDOW_BITS) as f32 + 0.5).floor() as i16
            })
            .collect();
        let one = (1u32 << NOISE_REDUCTION_BITS) as f32;
        Self {
            coefficients,
            input: vec![0; WINDOW],
            input_used: 0,
            fft: RealFft::new(),
            fft_input: Box::new([0; FFT_SIZE]),
            fft_output: Box::new([Complex::default(); SPECTRUM]),
            filterbank: Filterbank::new(),
            energy: vec![0; SPECTRUM + 2 * CHANNEL_BLOCK],
            work: [0; CHANNELS + 1],
            estimate: [0; CHANNELS],
            even_smoothing: (EVEN_SMOOTHING * one) as u16 as u32,
            odd_smoothing: (ODD_SMOOTHING * one) as u16 as u32,
            min_signal_remaining: (MIN_SIGNAL_REMAINING * one) as u16 as u32,
            gain_lut: gain_lut(),
        }
    }

    /// Forgets the buffered samples and the noise estimate.
    pub fn reset(&mut self) {
        self.input.fill(0);
        self.input_used = 0;
        self.estimate = [0; CHANNELS];
    }

    /// Takes samples from the front of `samples` until a window is complete or
    /// they run out. Returns how many it took, and the feature frame when a
    /// window completed (call again with the rest).
    pub fn process(&mut self, samples: &[i16]) -> (usize, Option<[u16; CHANNELS]>) {
        let take = (WINDOW - self.input_used).min(samples.len());
        self.input[self.input_used..self.input_used + take].copy_from_slice(&samples[..take]);
        self.input_used += take;
        if self.input_used < WINDOW {
            return (take, None);
        }
        (take, Some(self.frame()))
    }

    fn frame(&mut self) -> [u16; CHANNELS] {
        // Window the input, noting the largest magnitude.
        let mut max_abs: i16 = 0;
        for i in 0..WINDOW {
            let value = ((i32::from(self.input[i]) * i32::from(self.coefficients[i]))
                >> WINDOW_BITS) as i16;
            self.fft_input[i] = value;
            // -32768 stays negative, as it does in the original's 16-bit negation.
            let magnitude = if value < 0 {
                value.wrapping_neg()
            } else {
                value
            };
            max_abs = max_abs.max(magnitude);
        }
        self.input.copy_within(STEP.., 0);
        self.input_used -= STEP;

        // Scale the window up so the fixed-point FFT keeps as many bits as it can.
        let input_shift = 15 - msb32(max_abs as i32 as u32).min(15);
        for v in self.fft_input[..WINDOW].iter_mut() {
            *v = ((*v as u16) << input_shift) as i16;
        }
        self.fft_input[WINDOW..].fill(0);
        self.fft.forward(&self.fft_input, &mut self.fft_output);

        let fb = &self.filterbank;
        for i in fb.start_index..fb.end_index {
            let (real, imag) = (
                i32::from(self.fft_output[i].r),
                i32::from(self.fft_output[i].i),
            );
            self.energy[i] = (real * real).wrapping_add(imag * imag) as u32;
        }

        // Each bin feeds the channel below it by its weight and the one above by the rest.
        let mut weight_accumulator: u64 = 0;
        let mut unweight_accumulator: u64 = 0;
        for chan in 0..=CHANNELS {
            let (bin, at) = (fb.frequency_starts[chan], fb.weight_starts[chan]);
            for j in 0..fb.widths[chan] {
                // The original reads the energy as a signed 32-bit value before widening it.
                let magnitude = i64::from(self.energy[bin + j] as i32) as u64;
                weight_accumulator = weight_accumulator
                    .wrapping_add((i64::from(fb.weights[at + j]) as u64).wrapping_mul(magnitude));
                unweight_accumulator = unweight_accumulator
                    .wrapping_add((i64::from(fb.unweights[at + j]) as u64).wrapping_mul(magnitude));
            }
            self.work[chan] = weight_accumulator;
            weight_accumulator = unweight_accumulator;
            unweight_accumulator = 0;
        }

        let mut out = [0u16; CHANNELS];
        for (i, o) in out.iter_mut().enumerate() {
            let mut signal = sqrt64(self.work[i + 1]) >> input_shift;

            // Noise reduction: track a smoothed estimate per channel and subtract it.
            let smoothing = if i & 1 == 0 {
                self.even_smoothing
            } else {
                self.odd_smoothing
            };
            let one_minus_smoothing = (1u32 << NOISE_REDUCTION_BITS) - smoothing;
            let signal_scaled_up = signal.wrapping_shl(SMOOTHING_BITS);
            let mut estimate = ((u64::from(signal_scaled_up) * u64::from(smoothing)
                + u64::from(self.estimate[i]) * u64::from(one_minus_smoothing))
                >> NOISE_REDUCTION_BITS) as u32;
            self.estimate[i] = estimate;
            estimate = estimate.min(signal_scaled_up);
            let floor = ((u64::from(signal) * u64::from(self.min_signal_remaining))
                >> NOISE_REDUCTION_BITS) as u32;
            let subtracted = (signal_scaled_up - estimate) >> SMOOTHING_BITS;
            signal = subtracted.max(floor);

            // Per-channel automatic gain from the noise estimate.
            let gain = i32::from(wide_dynamic_function(self.estimate[i], &self.gain_lut)) as u32;
            let snr_shift = PCAN_GAIN_BITS - CORRECTION_BITS - PCAN_SNR_BITS;
            let snr = ((u64::from(signal) * u64::from(gain)) >> snr_shift) as u32;
            signal = pcan_shrink(snr);

            // Log scale.
            let value = signal.wrapping_shl(CORRECTION_BITS);
            let value = if value > 1 { log(value) } else { 0 };
            *o = value.min(0xFFFF) as u16;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integer_square_roots() {
        assert_eq!(sqrt32(0), 0);
        assert_eq!(sqrt32(16), 4);
        assert_eq!(sqrt32(17), 4);
        assert_eq!(sqrt32(1_000_000), 1000);
        assert_eq!(sqrt64(1 << 40), 1 << 20);
        assert_eq!(sqrt64(10_000_000_000), 100_000);
    }

    #[test]
    fn the_log_is_the_natural_log_times_64() {
        for x in [2u32, 10, 1000, 123_456, 4_000_000_000] {
            let want = (f64::from(x).ln() * 64.0).round() as i64;
            assert!(
                (i64::from(log(x)) - want).abs() <= 1,
                "{x}: {} vs {want}",
                log(x)
            );
        }
    }

    #[test]
    fn the_filterbank_covers_the_band() {
        let fb = Filterbank::new();
        // 125 Hz is bin 4 of 31.25 Hz bins; the first bin used is the next one.
        assert_eq!(fb.start_index, 5);
        assert!(
            fb.end_index <= SPECTRUM && fb.end_index > 230,
            "{}",
            fb.end_index
        );
        assert!(fb.weights.iter().all(|w| (0..=4096).contains(w)));
    }
}
