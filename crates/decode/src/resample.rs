//! Sample rate conversion: a windowed-sinc polyphase resampler.
//!
//! The method is bandlimited interpolation as Julius O. Smith describes it in
//! "Digital Audio Resampling Home Page"
//! (<https://ccrma.stanford.edu/~jos/resample/>, the pages "Theory of Ideal
//! Bandlimited Interpolation" and "Implementation", read 2026-10-03): an output
//! sample at time `t` is the sum over input samples `x[n]` of `x[n] h(t - n)`,
//! where `h` is the ideal lowpass (a sinc) cut to finite length by a Kaiser
//! window; when the rate goes down the same filter is stretched so that its
//! cutoff follows the lower of the two rates (Smith: "the lowpass cutoff must
//! be placed below half the new lower sampling rate", and the kernel becomes
//! `min{1, Fs'/Fs} sinc(min{Fs, Fs'} t)`). The Kaiser window and its Bessel
//! series are as Smith's "Spectral Audio Signal Processing" gives them, "Kaiser
//! Window" (<https://ccrma.stanford.edu/~jos/sasp/Kaiser_Window.html>, read
//! 2026-10-03). The two design estimates used to pick the window's shape and
//! the transition width (`beta = 0.1102 (A - 8.7)` and
//! `width = (A - 7.95) / (14.36 taps)`) are Kaiser's empirical formulas,
//! `ASSUMED` from memory and not re-read; nothing rests on them, because the
//! result is measured: the tests in `tests/resampler_quality.rs` and
//! `docs/measurements/resampler-quality.md` hold the figures.
//!
//! The rates chorus meets are in rational ratios, so the filter is stored as
//! one row of taps per output phase (a polyphase filter): for 44.1 to 48 kHz
//! there are 160 phases. A ratio with more than [`MAX_EXACT_PHASES`] phases
//! uses that many rows and interpolates linearly between neighbouring rows,
//! Smith's table-with-interpolation form.
//!
//! No clock is read here: time is counted in frames.

/// Zero crossings of the sinc kept on each side, in samples of the lower rate.
const ZERO_CROSSINGS: usize = 96;
/// The stopband attenuation the Kaiser window is designed for, in dB.
const DESIGN_ATTENUATION_DB: f64 = 110.0;
/// A ratio with more output phases than this interpolates between rows.
const MAX_EXACT_PHASES: u64 = 1024;

/// A resampler for one stream of interleaved frames.
///
/// The output is aligned with the input: output frame `k` is the input at time
/// `k * from / to` frames, and `n` input frames give exactly
/// `ceil(n * to / from)` output frames once [`Resampler::flush`] has run. So
/// nothing needs trimming, and a track's length after resampling is known
/// before it is decoded.
pub struct Resampler {
    channels: usize,
    /// Output phases per input frame (`to / gcd`).
    l: u64,
    /// Phase steps per output frame (`from / gcd`).
    m: u64,
    /// Taps on each side of the centre, in input frames.
    half: usize,
    /// Rows in `table`, not counting the one extra row at phase 1.
    rows: usize,
    /// `rows + 1` rows of `2 * half` taps.
    table: Vec<f32>,
    /// Input frames not yet consumed. Frame `start` is input frame
    /// `position - half + 1`; before the first input that is silence.
    buf: Vec<f32>,
    start: usize,
    /// The phase of the next output, in `1/l` of an input frame.
    frac: u64,
    /// The input frame the next output falls in.
    position: u64,
    /// Input frames taken since the start or the last flush.
    taken: u64,
    passthrough: bool,
    to: u32,
    from: u32,
}

impl Resampler {
    /// The lowest rate, on either side, the resampler is built and measured for.
    pub const MIN_RATE: u32 = 8_000;
    /// The highest rate, on either side.
    pub const MAX_RATE: u32 = 192_000;

    /// A resampler from `from` Hz to `to` Hz for `channels` interleaved
    /// channels.
    ///
    /// # Panics
    ///
    /// When a rate is outside [`Resampler::MIN_RATE`]`..=`[`Resampler::MAX_RATE`]
    /// or `channels` is zero: the caller checks a rate it read from a file
    /// before it asks for a resampler.
    pub fn new(from: u32, to: u32, channels: u16) -> Resampler {
        let range = Self::MIN_RATE..=Self::MAX_RATE;
        assert!(
            range.contains(&from) && range.contains(&to),
            "resampler rates must be within 8000..=192000 Hz, got {from} to {to}"
        );
        assert!(channels > 0, "a resampler needs at least one channel");
        let g = gcd(u64::from(from), u64::from(to));
        let l = u64::from(to) / g;
        let m = u64::from(from) / g;
        let ratio = f64::from(to) / f64::from(from);
        let low = ratio.min(1.0);
        let half = (ZERO_CROSSINGS as f64 / low).ceil() as usize;
        let taps = 2 * half;
        let rows = l.min(MAX_EXACT_PHASES) as usize;
        let passthrough = from == to;
        let mut table = Vec::new();
        if !passthrough {
            // Kaiser's design: the shape for the wanted attenuation, and the
            // transition width this length buys, in cycles per input frame.
            let beta = 0.1102 * (DESIGN_ATTENUATION_DB - 8.7);
            let transition = (DESIGN_ATTENUATION_DB - 7.95) / (14.36 * taps as f64);
            // The stopband starts at the lower rate's Nyquist frequency, so
            // nothing above it folds back; the passband ends one transition
            // width below.
            let cutoff = 0.5 * low - transition / 2.0;
            let i0_beta = bessel_i0(beta);
            table.reserve((rows + 1) * taps);
            for row in 0..=rows {
                let phase = row as f64 / rows as f64;
                let at = table.len();
                let mut sum = 0.0f64;
                for j in 0..taps {
                    // The distance from the output instant to input frame j of the window.
                    let t = phase + (half as f64 - 1.0) - j as f64;
                    let x = t / half as f64;
                    let h = if x.abs() >= 1.0 {
                        0.0
                    } else {
                        let window = bessel_i0(beta * (1.0 - x * x).sqrt()) / i0_beta;
                        2.0 * cutoff * sinc(2.0 * cutoff * t) * window
                    };
                    sum += h;
                    table.push(h as f32);
                }
                // Unity gain at 0 Hz in every phase.
                for h in &mut table[at..] {
                    *h = (f64::from(*h) / sum) as f32;
                }
            }
        }
        let channels = usize::from(channels);
        let mut resampler = Resampler {
            channels,
            l,
            m,
            half,
            rows,
            table,
            buf: Vec::new(),
            start: 0,
            frac: 0,
            position: 0,
            taken: 0,
            passthrough,
            to,
            from,
        };
        resampler.reset();
        resampler
    }

    fn reset(&mut self) {
        self.buf.clear();
        // The frames before the first input are silence.
        self.buf.resize((self.half - 1) * self.channels, 0.0);
        self.start = 0;
        self.frac = 0;
        self.position = 0;
        self.taken = 0;
    }

    /// Takes `input` (whole interleaved frames; a trailing partial frame is
    /// ignored) and appends to `out` every output frame it now has the input
    /// for. The last [`Resampler::delay_frames`] of output wait for more input
    /// or for [`Resampler::flush`].
    pub fn process(&mut self, input: &[f32], out: &mut Vec<f32>) {
        let whole = input.len() / self.channels * self.channels;
        let input = &input[..whole];
        if self.passthrough {
            out.extend_from_slice(input);
            return;
        }
        self.buf.extend_from_slice(input);
        self.taken += (whole / self.channels) as u64;
        self.run(out, u64::MAX);
    }

    /// Appends the output frames still owed for the input taken so far, as if
    /// silence followed, and starts over: the next input is a new stream.
    pub fn flush(&mut self, out: &mut Vec<f32>) {
        if self.passthrough {
            return;
        }
        let silence = self.buf.len() + self.half * self.channels;
        self.buf.resize(silence, 0.0);
        self.run(out, self.taken);
        self.reset();
    }

    /// How many output frames lag the input: [`Resampler::process`] has
    /// emitted every output frame except about this many, which need input not
    /// yet given. It is a latency only; the output is not shifted in time.
    pub fn delay_frames(&self) -> usize {
        if self.passthrough {
            return 0;
        }
        (self.half as u64 * u64::from(self.to)).div_ceil(u64::from(self.from)) as usize
    }

    /// Emits outputs while their whole window is buffered and they fall in an
    /// input frame before `end`.
    fn run(&mut self, out: &mut Vec<f32>, end: u64) {
        let ch = self.channels;
        let taps = 2 * self.half;
        let exact = self.rows as u64 == self.l;
        while self.position < end && self.buf.len() / ch - self.start >= taps {
            let window = &self.buf[self.start * ch..(self.start + taps) * ch];
            if exact {
                let row = &self.table[self.frac as usize * taps..][..taps];
                for c in 0..ch {
                    let mut acc = 0.0f64;
                    for (frame, h) in window.chunks_exact(ch).zip(row) {
                        acc += f64::from(frame[c]) * f64::from(*h);
                    }
                    out.push(acc as f32);
                }
            } else {
                // Between two stored phases: interpolate the taps linearly.
                let scaled = self.frac * self.rows as u64;
                let index = (scaled / self.l) as usize;
                let weight = (scaled % self.l) as f64 / self.l as f64;
                let lower = &self.table[index * taps..][..taps];
                let upper = &self.table[(index + 1) * taps..][..taps];
                for c in 0..ch {
                    let mut acc = 0.0f64;
                    for ((frame, a), b) in window.chunks_exact(ch).zip(lower).zip(upper) {
                        let h = f64::from(*a) + weight * (f64::from(*b) - f64::from(*a));
                        acc += f64::from(frame[c]) * h;
                    }
                    out.push(acc as f32);
                }
            }
            self.frac += self.m;
            let advance = self.frac / self.l;
            self.frac %= self.l;
            self.position += advance;
            self.start += advance as usize;
        }
        // Forget the input no output needs any more.
        let consumed = self.start.min(self.buf.len() / ch);
        self.buf.drain(..consumed * ch);
        self.start -= consumed;
    }
}

fn gcd(mut a: u64, mut b: u64) -> u64 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

fn sinc(x: f64) -> f64 {
    if x.abs() < 1e-12 {
        1.0
    } else {
        let px = std::f64::consts::PI * x;
        px.sin() / px
    }
}

/// The zeroth-order modified Bessel function of the first kind, by its power
/// series: the sum over k of ((x/2)^k / k!)^2.
fn bessel_i0(x: f64) -> f64 {
    let half = x / 2.0;
    let mut term = 1.0f64;
    let mut sum = 1.0f64;
    for k in 1..200 {
        term *= half / k as f64;
        let add = term * term;
        sum += add;
        if add < sum * 1e-18 {
            break;
        }
    }
    sum
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(rate: u32, hz: f64, frames: usize, channels: usize) -> Vec<f32> {
        let mut v = Vec::with_capacity(frames * channels);
        for i in 0..frames {
            let s = (2.0 * std::f64::consts::PI * hz * i as f64 / f64::from(rate)).sin() * 0.5;
            for c in 0..channels {
                v.push(if c % 2 == 0 { s as f32 } else { -s as f32 });
            }
        }
        v
    }

    #[test]
    fn bessel_i0_matches_known_values() {
        // I0(0) = 1; I0(1) = 1.2660658777520084 (Abramowitz and Stegun table 9.8, ASSUMED from
        // memory; the series itself is the definition and converges to it).
        assert!((bessel_i0(0.0) - 1.0).abs() < 1e-15);
        assert!((bessel_i0(1.0) - 1.266_065_877_752_008_4).abs() < 1e-12);
    }

    #[test]
    fn the_output_length_is_exact_for_every_chunking() {
        for (from, to) in [
            (44_100, 48_000),
            (96_000, 48_000),
            (48_000, 44_100),
            (8_000, 48_000),
            (44_100, 47_999),
        ] {
            for frames in [1usize, 441, 1000, 4410] {
                let input = tone(from, 440.0, frames, 2);
                let mut whole = Vec::new();
                let mut r = Resampler::new(from, to, 2);
                r.process(&input, &mut whole);
                r.flush(&mut whole);
                let want = (frames as u64 * u64::from(to)).div_ceil(u64::from(from)) as usize;
                assert_eq!(whole.len() / 2, want, "{from} to {to}, {frames} frames");
                // In chunks of 7 frames: the same samples, bit for bit.
                let mut chunked = Vec::new();
                for chunk in input.chunks(14) {
                    r.process(chunk, &mut chunked);
                }
                r.flush(&mut chunked);
                assert_eq!(chunked, whole, "{from} to {to}, {frames} frames, chunked");
            }
        }
    }

    #[test]
    fn process_holds_back_no_more_than_the_stated_delay() {
        let mut r = Resampler::new(44_100, 48_000, 1);
        let mut out = Vec::new();
        r.process(&tone(44_100, 1000.0, 4410, 1), &mut out);
        let total = 4800;
        assert!(
            out.len() <= total && total - out.len() <= r.delay_frames() + 1,
            "{} of {total}",
            out.len()
        );
    }

    #[test]
    fn equal_rates_pass_through_untouched() {
        let mut r = Resampler::new(48_000, 48_000, 2);
        let input = tone(48_000, 997.0, 100, 2);
        let mut out = Vec::new();
        r.process(&input, &mut out);
        r.flush(&mut out);
        assert_eq!(out, input);
        assert_eq!(r.delay_frames(), 0);
    }

    #[test]
    fn channels_stay_separate() {
        let mut r = Resampler::new(48_000, 44_100, 2);
        let mut input = Vec::new();
        for i in 0..4800 {
            input.push(
                (2.0 * std::f64::consts::PI * 1000.0 * f64::from(i) / 48_000.0).sin() as f32 * 0.5,
            );
            input.push(0.0);
        }
        let mut out = Vec::new();
        r.process(&input, &mut out);
        r.flush(&mut out);
        assert!(out.as_chunks::<2>().0.iter().all(|f| f[1] == 0.0));
        assert!(out.as_chunks::<2>().0.iter().any(|f| f[0].abs() > 0.4));
    }

    #[test]
    #[should_panic(expected = "resampler rates must be within")]
    fn a_rate_outside_the_range_is_refused_loudly() {
        let _ = Resampler::new(4_000, 48_000, 2);
    }
}
