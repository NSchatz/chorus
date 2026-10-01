//! The visualizer analysis: levels, bands, beat and colour from the audio a
//! slot streams (goal 12, K65; `docs/visualizer.md`).
//!
//! # What it computes
//!
//! From interleaved samples (full scale 1.0), on an analysis grid of one hop
//! every [`HOP_MS`] ms, a [`Frame`] every [`FRAME_HOPS`] hops carrying:
//!
//! - **peak**: the sample peak over the frame's period, over every channel,
//!   in dBFS, held with a peak programme meter's fall ([`FALL_DB_PER_S`]) and
//!   mapped to a byte by [`level_byte`];
//! - **bands**: [`BANDS`] sixth-octave band levels (IEC 61260-1, below) from
//!   a short-time Fourier transform of the channels' mean, each band's power
//!   averaged over the frame and held with the same fall; [`Frame::bands`]
//!   merges them into the 0 to 64 bands an endpoint asked for
//!   (`visualizer_bands`, docs/protocol.md);
//! - **beat**: an onset strength, 0 for none, from Dixon's spectral flux and
//!   his peak-picking conditions, run causally (below);
//! - **colour**, now and then: the hue from the spectral centroid, the
//!   saturation from the spectral flatness, the brightness from the level
//!   ([`Colour`]).
//!
//! The research these rest on is `docs/research/research-dsp-phase-b.md`
//! section 2 (read 2026-10-01); each value below names its source or says
//! ASSUMED.
//!
//! # The onset detector
//!
//! S. Dixon, "Onset Detection Revisited", Proc. DAFx-06, Montreal, 2006,
//! <https://www.dafx.de/paper-archive/2006/papers/p_133.pdf> (read
//! 2026-10-01). Section 2: an STFT "using a Hamming window", "window size
//! N = 2048 (46 ms at a sampling rate of r = 44100 Hz) and hop size h = 441
//! (10 ms ...)", "calculated at a frame rate of 100 Hz". Section 2.1:
//! `SF(n) = sum_k H(|X(n,k)| - |X(n-1,k)|)`, `H(x) = (x + |x|) / 2`, the
//! L1 norm of linear magnitudes. Section 2.6: with `f` normalised to mean 0
//! and deviation 1, hop `n` is an onset when `f(n) >= f(k)` for
//! `n - w <= k <= n + w` and `f(n)` is at least the mean of `f` over
//! `n - m w .. n + w` plus `delta`, with `w = 3`, `m = 3`. His third
//! condition (`g_alpha`) is left out: "the improvement in results due to the
//! use of the function g_alpha(n) was marginal, assuming a suitable value for
//! delta is chosen". A stream has no whole function to normalise, so the
//! mean and deviation are running ones ([`ONSET_NORM_S`]), and every hop in
//! the picking window is normalised by the same ones, as his whole function
//! is by one: the local maximum is then the raw flux's.
//!
//! # Time
//!
//! Nothing here reads a clock. Every frame names the instant it describes as
//! a sample index ([`Frame::at_sample`]) on the analyser's own count of the
//! sample frames it was handed, which the server maps onto its timeline
//! (the slots' grid is in samples too). Analysis hop `n` is centred on
//! sample `n * hop`, as Dixon's STFT is, so it needs `N/2` samples past its
//! instant, and the peak picking needs [`ONSET_W`] hops past that: a frame
//! comes out about `N/2 + (W + FRAME_HOPS) * hop` after the audio it
//! describes was handed in (91 ms at 48 kHz), inside the 180 ms an endpoint
//! plays behind the server (the wired playout latency, `config/sync.conf`).
//!
//! # Determinism
//!
//! The same samples in any pieces give the same frames: the analysis grid is
//! fixed by the sample count, not by how the samples arrive. `f32` samples,
//! `f64` arithmetic, no allocation after [`Analyzer::new`].

use std::f64::consts::PI;

/// The analysis hop, ms: Dixon's frame rate of 100 Hz (DAFx-06 section 2).
pub const HOP_MS: u32 = 10;

/// Analysis hops per [`Frame`]: 4, 25 frames a second. The precedent is
/// WLED's audio sync, "one packet every 20 milliseconds (approx)"
/// (<https://mm.kno.wled.ge/soundreactive/sync/>, read 2026-10-01); chorus
/// sends half that rate because a frame arrives up to the wireless tier's
/// 500 ms playout latency before it is heard and the status LED holds 16
/// events ahead (`crates/controls/src/led.rs` `QUEUE`, and its C twin): 25
/// a second fill 12.5 of them, 50 would overflow it. ASSUMED.
pub const FRAME_HOPS: u64 = 4;

/// The analysis window, s: Dixon's 2048 samples at 44.1 kHz. The length used
/// is the power of two nearest this many samples at the stream's rate (2048
/// at 44.1 and 48 kHz, 1024 at 16 kHz, 4096 at 96 kHz).
pub const WINDOW_S: f64 = 2048.0 / 44_100.0;

/// The bands analysed: the sixth-octave bands of IEC 61260-1:2014 from
/// 21.1 Hz to 18.8 kHz. Clause 5.2.1: octave ratio `G = 10^(3/10)`; 5.3:
/// reference 1000 Hz; 5.4.2, for even `b`: `fm = 1000 G^((2x + 1) / (2b))`;
/// 3.11: edges at `fm G^(-1/(2b))` and `fm G^(1/(2b))`
/// (<https://cdn.standards.iteh.ai/samples/13383/3c4ae3e762b540cc8111744cb8f0ae8e/IEC-61260-1-2014.pdf>,
/// preview pages read 2026-10-01). `b = 6`, `x = -34 ..= 25`. An endpoint
/// asking for more (the wire allows 64) gets these 60.
pub const BANDS: usize = 60;

/// The lowest band's `x` in the formula of [`BANDS`].
const BAND_X0: i32 = -34;

/// The level shown as 0, dBFS; 0 dBFS is 255. ASSUMED: a 60 dB span (the
/// research's rule, `docs/research/research-dsp-phase-b.md` 2c).
pub const FLOOR_DB: f64 = -60.0;

/// How fast the peak and the band levels fall, dB per second, after their
/// input does (the attack is instant): the EBU peak programme meter's return
/// time, "in 2.8 +-0.3 s" from +12 to -12 (24 dB) in normal mode, EBU Tech
/// 3205-E, 2nd edition 1979 (<https://tech.ebu.ch/docs/tech/tech3205.pdf>,
/// read 2026-10-01). Its transfer from a meter to bands is ASSUMED.
pub const FALL_DB_PER_S: f64 = 24.0 / 2.8;

/// Dixon's local-maximum window, hops (`w = 3`, DAFx-06 section 2.6).
pub const ONSET_W: usize = 3;
/// Dixon's mean multiplier (`m = 3`, section 2.6).
pub const ONSET_M: usize = 3;
/// Dixon's threshold above the local mean, `delta`, in deviations.
/// ASSUMED: the paper tunes it per data set and prints no value; 0.5 keeps
/// a steady tone and a sweep from beating (the fixtures say so).
pub const ONSET_DELTA: f64 = 0.5;
/// The running mean and deviation's time constant, s. ASSUMED: 3 s, the EBU
/// short-term loudness window (EBU Tech 3341), as the research suggests.
pub const ONSET_NORM_S: f64 = 3.0;
/// The least spectral flux an onset needs, and the least deviation the flux
/// is normalised by, relative to a full-scale sine's bin magnitude.
/// ASSUMED: -60 dB, so silence and dither never beat.
pub const ONSET_FLOOR: f64 = 0.001;
/// Beat strength per deviation above the running mean: `min(255, round(64
/// z))`. ASSUMED (the research's scale); the status LED flashes at 128 and
/// above (`crates/controls/src/led.rs` `BEAT_THRESHOLD`), two deviations.
pub const BEAT_PER_DEVIATION: f64 = 64.0;

/// Hops held for the peak picking: the local mean's reach before a hop and
/// the local maximum's after it.
const HELD: usize = (ONSET_M + 1) * ONSET_W + 1;

/// Colours go out at most this often, ms, and fade over the same time.
/// ASSUMED: twice a second, inside every bound the research found: WCAG 2.2
/// success criterion 2.3.1, nothing "flashes more than three times in any
/// one second period"
/// (<https://www.w3.org/WAI/WCAG22/Understanding/three-flashes-or-below-threshold.html>,
/// read 2026-10-01; a screen rule applied to lamps), and the smart lights'
/// command rates (Nanoleaf's 10 Hz, Hue's effect rate below 12.5 Hz, both
/// LEADs in the research).
pub const COLOUR_INTERVAL_MS: u32 = 500;
/// A colour goes out only when its hue moved this far, degrees, its
/// saturation [`COLOUR_SATURATION_STEP`], or its brightness
/// [`COLOUR_BRIGHTNESS_STEP`]. ASSUMED.
pub const COLOUR_HUE_STEP: f64 = 15.0;
/// See [`COLOUR_HUE_STEP`]. ASSUMED.
pub const COLOUR_SATURATION_STEP: f64 = 0.1;
/// See [`COLOUR_HUE_STEP`]. ASSUMED.
pub const COLOUR_BRIGHTNESS_STEP: u8 = 16;
/// The spectral centroids mapped to the ends of the hue range, Hz: blue at
/// [`CENTROID_LOW_HZ`], red at [`CENTROID_HIGH_HZ`], log in between. The
/// direction is E. Richan and J. Rouat's, "The spectral centroid (measuring
/// timbral brightness) is mapped to a gradient from blue to red and spectral
/// flatness (measuring tonality) is then mapped to the color's saturation"
/// ("A proposal and evaluation of new timbre visualisation methods for audio
/// sample browsers", Personal and Ubiquitous Computing 2020, section 4,
/// <https://arxiv.org/pdf/2011.15096>, read 2026-10-01). The range is
/// ASSUMED.
pub const CENTROID_LOW_HZ: f64 = 100.0;
/// See [`CENTROID_LOW_HZ`]. ASSUMED.
pub const CENTROID_HIGH_HZ: f64 = 8_000.0;
/// The hue at [`CENTROID_LOW_HZ`], degrees (blue); [`CENTROID_HIGH_HZ`] is 0
/// (red).
pub const HUE_LOW: f64 = 240.0;
/// The colour's time constant, s: the 0.4 s of ITU-R BS.1771's momentary
/// meter as EBU Tech 3341 V4 reports it, "a 1st order IIR filter with a
/// time-constant of 0.4 s" (<https://tech.ebu.ch/docs/tech/tech3341.pdf>,
/// read 2026-10-01). Unweighted here: no K-weighting (ASSUMED, a follow-up).
pub const COLOUR_SMOOTH_S: f64 = 0.4;

/// A colour for an endpoint's lights (the wire's `color`, 0x35).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Colour {
    /// Red, 0 to 255.
    pub red: u8,
    /// Green.
    pub green: u8,
    /// Blue.
    pub blue: u8,
    /// Brightness, 0 off to 255.
    pub brightness: u8,
    /// Fade time from the previous colour, ms.
    pub transition_ms: u16,
}

/// One visualizer frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frame {
    /// The instant it describes, as a sample index on the analyser's count:
    /// the start of its period, or the onset's hop when it carries a beat.
    pub at_sample: u64,
    /// The held sample peak, 0 (at or below [`FLOOR_DB`]) to 255 (full
    /// scale).
    pub peak: u8,
    /// Beat strength, 0 for none.
    pub beat: u8,
    /// Band levels, dBFS, low to high, held.
    pub band_db: [f32; BANDS],
    /// A new colour, when one is due ([`COLOUR_INTERVAL_MS`]).
    pub colour: Option<Colour>,
}

impl Frame {
    /// Whether it shows nothing: no peak, no beat, every band at the floor.
    pub fn is_silent(&self) -> bool {
        self.peak == 0 && self.beat == 0 && self.band_db.iter().all(|d| f64::from(*d) <= FLOOR_DB)
    }

    /// The band levels merged into `n` bands (more than [`BANDS`] is taken as
    /// [`BANDS`]), each a [`level_byte`], into `out` (cleared first). Output
    /// band `j` sums the power of analysed bands `j * 60 / n` up to
    /// `(j + 1) * 60 / n` (IEC 61260-1 5.4 Note 1: narrow bands "can be
    /// combined to approximate the band level" of a wider one), so 30 bands
    /// are the third-octave bands. Allocates nothing when `out` has room.
    pub fn bands(&self, n: usize, out: &mut Vec<u8>) {
        out.clear();
        let n = n.min(BANDS);
        for j in 0..n {
            let lo = j * BANDS / n;
            let hi = ((j + 1) * BANDS / n).max(lo + 1);
            let power: f64 = self.band_db[lo..hi]
                .iter()
                .filter(|d| f64::from(**d) > FLOOR_DB)
                .map(|d| 10f64.powf(f64::from(*d) / 10.0))
                .sum();
            out.push(level_byte(db(power)));
        }
    }
}

/// A power ratio in dB, [`FLOOR_DB`] for none.
fn db(power: f64) -> f64 {
    if power > 0.0 {
        (10.0 * power.log10()).max(FLOOR_DB)
    } else {
        FLOOR_DB
    }
}

/// A level in dBFS as a byte: `clamp(round(255 (dBFS + 60) / 60), 0, 255)`
/// (the research's rule; the 60 dB is [`FLOOR_DB`], ASSUMED).
pub fn level_byte(db: f64) -> u8 {
    if db.is_nan() || db <= FLOOR_DB {
        return 0;
    }
    ((db - FLOOR_DB) / -FLOOR_DB * 255.0)
        .round()
        .clamp(0.0, 255.0) as u8
}

/// The centre frequency of analysed band `b`, Hz (IEC 61260-1 5.4.2, `b` =
/// 6).
pub fn band_centre_hz(b: usize) -> f64 {
    let x = BAND_X0 + b as i32;
    1_000.0 * 10f64.powf(0.3 * f64::from(2 * x + 1) / 12.0)
}

/// The edges of analysed band `b`, Hz (IEC 61260-1 3.11).
pub fn band_edges_hz(b: usize) -> (f64, f64) {
    let half = 10f64.powf(0.3 / 12.0);
    let fm = band_centre_hz(b);
    (fm / half, fm * half)
}

/// HSV to RGB, each 0 to 255, hue in degrees, saturation and value 0 to 1:
/// the hexcone model of A. R. Smith, "Color Gamut Transform Pairs",
/// SIGGRAPH 1978, as written out at
/// <https://en.wikipedia.org/wiki/HSL_and_HSV#HSV_to_RGB> (read 2026-10-01).
pub fn hsv_to_rgb(hue: f64, saturation: f64, value: f64) -> (u8, u8, u8) {
    let h = hue.rem_euclid(360.0) / 60.0;
    let c = value * saturation;
    let x = c * (1.0 - (h % 2.0 - 1.0).abs());
    let (r, g, b) = match h as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = value - c;
    let byte = |v: f64| ((v + m) * 255.0).round().clamp(0.0, 255.0) as u8;
    (byte(r), byte(g), byte(b))
}

/// An in-place radix-2 FFT of a fixed power-of-two size, its twiddles and
/// bit reversal computed once.
#[derive(Debug, Clone)]
struct Fft {
    n: usize,
    cos: Vec<f64>,
    sin: Vec<f64>,
    reversed: Vec<usize>,
}

impl Fft {
    fn new(n: usize) -> Fft {
        let bits = n.trailing_zeros();
        Fft {
            n,
            cos: (0..n / 2)
                .map(|k| (2.0 * PI * k as f64 / n as f64).cos())
                .collect(),
            sin: (0..n / 2)
                .map(|k| -(2.0 * PI * k as f64 / n as f64).sin())
                .collect(),
            reversed: (0..n)
                .map(|i| i.reverse_bits() >> (usize::BITS - bits))
                .collect(),
        }
    }

    /// The forward transform of `re + j im`, in place.
    fn forward(&self, re: &mut [f64], im: &mut [f64]) {
        for i in 0..self.n {
            let j = self.reversed[i];
            if j > i {
                re.swap(i, j);
                im.swap(i, j);
            }
        }
        let mut len = 2;
        while len <= self.n {
            let step = self.n / len;
            for start in (0..self.n).step_by(len) {
                for k in 0..len / 2 {
                    let (wr, wi) = (self.cos[k * step], self.sin[k * step]);
                    let (a, b) = (start + k, start + k + len / 2);
                    let tr = re[b] * wr - im[b] * wi;
                    let ti = re[b] * wi + im[b] * wr;
                    re[b] = re[a] - tr;
                    im[b] = im[a] - ti;
                    re[a] += tr;
                    im[a] += ti;
                }
            }
            len *= 2;
        }
    }
}

/// One analysed hop, held until the peak picking can decide it.
#[derive(Debug, Clone, Copy)]
struct Hop {
    index: u64,
    /// Sample peak over the hop's own samples, every channel.
    peak: f32,
    /// Band powers relative to a full-scale sine's.
    band_power: [f64; BANDS],
    /// The spectral flux, relative to a full-scale sine's bin magnitude.
    flux: f64,
    /// `sum f P`, `sum P` and the spectral flatness over the analysed range.
    centroid_moment: f64,
    centroid_power: f64,
    flatness: f64,
}

/// What a frame gathers from its hops.
#[derive(Debug, Clone, Copy)]
struct Gather {
    first: u64,
    hops: u32,
    at_sample: u64,
    peak: f32,
    beat: u8,
    band_power: [f64; BANDS],
    centroid_moment: f64,
    centroid_power: f64,
    flatness: f64,
}

/// The analyser for one stream. See the module documentation.
#[derive(Debug, Clone)]
pub struct Analyzer {
    rate: u32,
    hop: u64,
    n: usize,
    fft: Fft,
    window: Vec<f64>,
    /// Bins `[lo, hi)` of each band; empty for a band above Nyquist.
    band_bins: [(usize, usize); BANDS],
    /// Bins `[lo, hi)` of the analysed range, for centroid and flatness.
    range_bins: (usize, usize),
    bin_hz: f64,
    /// The power of a full-scale sine wholly inside one band.
    power_ref: f64,
    /// The bin magnitude of a full-scale sine.
    magnitude_ref: f64,
    /// The channels' mean and peak of the last N sample frames.
    mono: Vec<f32>,
    peaks: Vec<f32>,
    /// Sample frames handed in so far (the analyser's count).
    total: u64,
    next_hop: u64,
    re: Vec<f64>,
    im: Vec<f64>,
    last_magnitude: Vec<f64>,
    have_last: bool,
    mean: f64,
    var: f64,
    norm_k: f64,
    held: Vec<Hop>,
    gather: Option<Gather>,
    shown_peak_db: f64,
    shown_db: [f64; BANDS],
    fall_per_frame: f64,
    colour_k: f64,
    smooth_position: f64,
    smooth_flatness: f64,
    smooth_power: f64,
    colour_started: bool,
    /// The last colour sent: hue, saturation, brightness, and when.
    sent: Option<(f64, f64, u8, u64)>,
}

impl Analyzer {
    /// An analyser for a stream at `sample_rate_hz`; `None` outside the
    /// wire's 8 to 384 kHz (docs/protocol.md).
    pub fn new(sample_rate_hz: u32) -> Option<Analyzer> {
        if !(8_000..=384_000).contains(&sample_rate_hz) {
            return None;
        }
        let rate = f64::from(sample_rate_hz);
        let hop = (rate * f64::from(HOP_MS) / 1_000.0).round() as u64;
        let n = 1usize << (WINDOW_S * rate).log2().round() as u32;
        // Dixon's Hamming window (DAFx-06 section 2).
        let window: Vec<f64> = (0..n)
            .map(|i| 0.54 - 0.46 * (2.0 * PI * i as f64 / (n - 1) as f64).cos())
            .collect();
        let sum_w: f64 = window.iter().sum();
        let sum_w2: f64 = window.iter().map(|w| w * w).sum();
        // A sine of amplitude 1: its windowed energy is sum_w2 / 2, the
        // DFT's is N times that (Parseval), and half is in the positive bins.
        let power_ref = n as f64 * sum_w2 / 4.0;
        let magnitude_ref = sum_w / 2.0;
        let bin_hz = rate / n as f64;
        let top = n / 2;
        // The first bin at or above `hz`; an edge that falls on a bin (1 kHz
        // does, at 16 kHz) must not be lost to rounding between two bands.
        let bin_of = |hz: f64| ((hz / bin_hz - 1e-9).ceil() as usize).min(top + 1);
        let mut band_bins = [(0usize, 0usize); BANDS];
        for (b, bins) in band_bins.iter_mut().enumerate() {
            let (lo_hz, hi_hz) = band_edges_hz(b);
            let (lo, hi) = (bin_of(lo_hz), bin_of(hi_hz));
            // A band narrower than a bin takes the bin nearest its centre,
            // while that bin is below Nyquist.
            let centre = (band_centre_hz(b) / bin_hz).round() as usize;
            *bins = if hi > lo {
                (lo, hi)
            } else if centre <= top {
                (centre, centre + 1)
            } else {
                (0, 0)
            };
        }
        let range_bins = (
            bin_of(band_edges_hz(0).0).max(1),
            bin_of(band_edges_hz(BANDS - 1).1),
        );
        let hops_per_s = rate / hop as f64;
        let frames_per_s = hops_per_s / FRAME_HOPS as f64;
        Some(Analyzer {
            rate: sample_rate_hz,
            hop,
            n,
            fft: Fft::new(n),
            window,
            band_bins,
            range_bins,
            bin_hz,
            power_ref,
            magnitude_ref,
            mono: vec![0.0; n],
            peaks: vec![0.0; n],
            total: 0,
            next_hop: 0,
            re: vec![0.0; n],
            im: vec![0.0; n],
            last_magnitude: vec![0.0; top + 1],
            have_last: false,
            mean: 0.0,
            var: 0.0,
            norm_k: 1.0 - (-1.0 / (ONSET_NORM_S * hops_per_s)).exp(),
            held: Vec::with_capacity(HELD),
            gather: None,
            shown_peak_db: FLOOR_DB,
            shown_db: [FLOOR_DB; BANDS],
            fall_per_frame: FALL_DB_PER_S / frames_per_s,
            colour_k: 1.0 - (-1.0 / (COLOUR_SMOOTH_S * frames_per_s)).exp(),
            smooth_position: 0.0,
            smooth_flatness: 0.0,
            smooth_power: 0.0,
            colour_started: false,
            sent: None,
        })
    }

    /// The stream's rate.
    pub fn sample_rate_hz(&self) -> u32 {
        self.rate
    }

    /// Samples per analysis hop.
    pub fn hop(&self) -> u64 {
        self.hop
    }

    /// The window length, samples.
    pub fn window_len(&self) -> usize {
        self.n
    }

    /// Sample frames handed in so far: the index the next one will have.
    pub fn position(&self) -> u64 {
        self.total
    }

    /// The analysed band holding `hz`, if any.
    /// Band `x` covers `1000 G^(x/6)` up to `1000 G^((x + 1)/6)`.
    pub fn band_of(&self, hz: f64) -> Option<usize> {
        if hz <= 0.0 {
            return None;
        }
        let x = (6.0 * (hz / 1_000.0).log10() / 0.3 + 1e-9).floor() as i64;
        let b = x - i64::from(BAND_X0);
        (0..BANDS as i64).contains(&b).then_some(b as usize)
    }

    /// Start again as if nothing had been heard, with the next sample frame
    /// at index `at` (the server does this when what it hands in stops being
    /// contiguous: a slot it skipped while nobody watched it).
    pub fn reset_at(&mut self, at: u64) {
        self.mono.fill(0.0);
        self.peaks.fill(0.0);
        self.total = at;
        self.next_hop = at.div_ceil(self.hop);
        self.last_magnitude.fill(0.0);
        self.have_last = false;
        self.mean = 0.0;
        self.var = 0.0;
        self.held.clear();
        self.gather = None;
        self.shown_peak_db = FLOOR_DB;
        self.shown_db = [FLOOR_DB; BANDS];
        self.colour_started = false;
        self.sent = None;
    }

    /// Hand in interleaved samples of `channels` channels (full scale 1.0; a
    /// trailing partial frame is ignored); every frame completed is pushed
    /// onto `out`.
    pub fn push(&mut self, samples: &[f32], channels: usize, out: &mut Vec<Frame>) {
        let channels = channels.max(1);
        for frame in samples.chunks_exact(channels) {
            let mut sum = 0f32;
            let mut peak = 0f32;
            for x in frame {
                let x = if x.is_finite() { *x } else { 0.0 };
                sum += x;
                peak = peak.max(x.abs());
            }
            let at = (self.total % self.n as u64) as usize;
            self.mono[at] = sum / channels as f32;
            self.peaks[at] = peak;
            self.total += 1;
            // Hop `k` is centred on sample `k * hop` and needs N/2 past it.
            if self.total == self.next_hop * self.hop + self.n as u64 / 2 {
                self.analyse(out);
                self.next_hop += 1;
            }
        }
    }

    fn ring(&self, s: u64) -> usize {
        (s % self.n as u64) as usize
    }

    fn analyse(&mut self, out: &mut Vec<Frame>) {
        let index = self.next_hop;
        let centre = index * self.hop;
        let start = centre as i64 - self.n as i64 / 2;
        for i in 0..self.n {
            let s = start + i as i64;
            // Before the first sample is silence (the ring starts zeroed).
            let x = if s < 0 {
                0.0
            } else {
                f64::from(self.mono[self.ring(s as u64)])
            };
            self.re[i] = x * self.window[i];
            self.im[i] = 0.0;
        }
        let mut peak = 0f32;
        for s in centre..centre + self.hop {
            peak = peak.max(self.peaks[self.ring(s)]);
        }
        self.fft.forward(&mut self.re, &mut self.im);
        // Dixon's SF (DAFx-06 section 2.1), the bin powers in `re`.
        let mut flux = 0.0;
        for k in 0..=self.n / 2 {
            let p = self.re[k] * self.re[k] + self.im[k] * self.im[k];
            let m = p.sqrt();
            if self.have_last {
                let rise = m - self.last_magnitude[k];
                if rise > 0.0 {
                    flux += rise;
                }
            }
            self.last_magnitude[k] = m;
            self.re[k] = p;
        }
        self.have_last = true;
        flux /= self.magnitude_ref;
        let mut band_power = [0.0; BANDS];
        for (b, (lo, hi)) in self.band_bins.iter().enumerate() {
            band_power[b] = self.re[*lo..*hi].iter().sum::<f64>() / self.power_ref;
        }
        // Centroid and flatness (geometric over arithmetic mean of the
        // power) over the bands' range, below Nyquist.
        let (lo, hi) = (self.range_bins.0, self.range_bins.1.min(self.n / 2 + 1));
        let (mut moment, mut power, mut logs) = (0.0, 0.0, 0.0);
        for k in lo..hi {
            let p = self.re[k] / self.power_ref;
            moment += p * k as f64 * self.bin_hz;
            power += p;
            logs += (p + 1e-30).ln();
        }
        let count = (hi - lo).max(1) as f64;
        let flatness = if power > 0.0 {
            ((logs / count).exp() / (power / count)).clamp(0.0, 1.0)
        } else {
            0.0
        };
        // The running statistics the detection function is normalised by.
        let d = flux - self.mean;
        self.mean += self.norm_k * d;
        self.var = (1.0 - self.norm_k) * (self.var + self.norm_k * d * d);

        if self.held.len() == HELD {
            self.held.remove(0);
        }
        self.held.push(Hop {
            index,
            peak,
            band_power,
            flux,
            centroid_moment: moment,
            centroid_power: power,
            flatness,
        });
        // Decide the hop W back, now that W after it are in hand.
        if self.held.len() > ONSET_W {
            let at = self.held.len() - 1 - ONSET_W;
            let beat = self.pick(at);
            let hop = self.held[at];
            self.emit(hop, beat, out);
        }
    }

    /// Dixon's first two conditions for the held hop at `at`, and the flux
    /// floor; the beat strength when they hold (module documentation).
    fn pick(&self, at: usize) -> u8 {
        let deviation = self.var.sqrt().max(ONSET_FLOOR);
        let f = |h: &Hop| (h.flux - self.mean) / deviation;
        let hop = self.held[at];
        let lo = at.saturating_sub(ONSET_W);
        let local_max = self.held[lo..=at + ONSET_W]
            .iter()
            .all(|h| hop.flux >= h.flux);
        let mean_lo = at.saturating_sub(ONSET_M * ONSET_W);
        let window = &self.held[mean_lo..=at + ONSET_W];
        let local_mean = window.iter().map(f).sum::<f64>() / window.len() as f64;
        let z = f(&hop);
        if local_max && z >= local_mean + ONSET_DELTA && hop.flux >= ONSET_FLOOR {
            (z * BEAT_PER_DEVIATION).round().clamp(1.0, 255.0) as u8
        } else {
            0
        }
    }

    /// One decided hop into the frame being gathered; a frame completes on
    /// its last hop of [`FRAME_HOPS`].
    fn emit(&mut self, hop: Hop, beat: u8, out: &mut Vec<Frame>) {
        let first = hop.index - hop.index % FRAME_HOPS;
        let g = self.gather.get_or_insert(Gather {
            first,
            hops: 0,
            at_sample: first * self.hop,
            peak: 0.0,
            beat: 0,
            band_power: [0.0; BANDS],
            centroid_moment: 0.0,
            centroid_power: 0.0,
            flatness: 0.0,
        });
        g.hops += 1;
        g.peak = g.peak.max(hop.peak);
        if beat > g.beat {
            g.beat = beat;
            g.at_sample = hop.index * self.hop;
        }
        for (sum, p) in g.band_power.iter_mut().zip(hop.band_power.iter()) {
            *sum += p;
        }
        g.centroid_moment += hop.centroid_moment;
        g.centroid_power += hop.centroid_power;
        g.flatness += hop.flatness;
        if hop.index % FRAME_HOPS != FRAME_HOPS - 1 {
            return;
        }
        let g = *g;
        self.gather = None;
        let hops = f64::from(g.hops.max(1));
        let mut band_db = [0f32; BANDS];
        for ((shown, out), power) in self
            .shown_db
            .iter_mut()
            .zip(band_db.iter_mut())
            .zip(g.band_power.iter())
        {
            *shown = (*shown - self.fall_per_frame).max(db(power / hops));
            *out = *shown as f32;
        }
        let peak_db = if g.peak > 0.0 {
            (20.0 * f64::from(g.peak).log10()).max(FLOOR_DB)
        } else {
            FLOOR_DB
        };
        self.shown_peak_db = (self.shown_peak_db - self.fall_per_frame).max(peak_db);
        let mut frame = Frame {
            at_sample: g.at_sample,
            peak: level_byte(self.shown_peak_db),
            beat: g.beat,
            band_db,
            colour: None,
        };
        frame.colour = self.colour(&g, hops, g.first * self.hop);
        out.push(frame);
    }

    /// The colour, smoothed, and whether one is due (module documentation
    /// and the constants: hue by log centroid from blue to red, saturation
    /// one minus flatness, brightness by level).
    fn colour(&mut self, g: &Gather, hops: f64, at: u64) -> Option<Colour> {
        let power = g.centroid_power / hops;
        let heard = g.centroid_power > 0.0;
        let position = if heard {
            let centroid = g.centroid_moment / g.centroid_power;
            ((centroid / CENTROID_LOW_HZ).max(1e-9).log2()
                / (CENTROID_HIGH_HZ / CENTROID_LOW_HZ).log2())
            .clamp(0.0, 1.0)
        } else {
            self.smooth_position
        };
        let flatness = if heard {
            g.flatness / hops
        } else {
            self.smooth_flatness
        };
        if self.colour_started {
            let k = self.colour_k;
            self.smooth_position += k * (position - self.smooth_position);
            self.smooth_flatness += k * (flatness - self.smooth_flatness);
            self.smooth_power += k * (power - self.smooth_power);
        } else if heard {
            self.smooth_position = position;
            self.smooth_flatness = flatness;
            self.smooth_power = power;
            self.colour_started = true;
        } else {
            return None;
        }
        let interval = u64::from(self.rate) * u64::from(COLOUR_INTERVAL_MS) / 1_000;
        if let Some((_, _, _, when)) = self.sent {
            if at < when + interval {
                return None;
            }
        }
        let hue = HUE_LOW * (1.0 - self.smooth_position);
        let saturation = (1.0 - self.smooth_flatness).clamp(0.0, 1.0);
        let brightness = level_byte(db(self.smooth_power).min(0.0));
        let due = match self.sent {
            None => brightness > 0,
            Some((h, s, b, _)) => {
                (h - hue).abs() >= COLOUR_HUE_STEP
                    || (s - saturation).abs() >= COLOUR_SATURATION_STEP
                    || b.abs_diff(brightness) >= COLOUR_BRIGHTNESS_STEP
            }
        };
        if !due {
            return None;
        }
        self.sent = Some((hue, saturation, brightness, at));
        let (red, green, blue) = hsv_to_rgb(hue, saturation, 1.0);
        Some(Colour {
            red,
            green,
            blue,
            brightness,
            transition_ms: COLOUR_INTERVAL_MS as u16,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(a: &mut Analyzer, x: &[f32]) -> Vec<Frame> {
        let mut out = Vec::new();
        a.push(x, 1, &mut out);
        out
    }

    fn sine(rate: u32, hz: f64, amplitude: f64, seconds: f64) -> Vec<f32> {
        (0..(f64::from(rate) * seconds) as usize)
            .map(|i| (amplitude * (2.0 * PI * hz * i as f64 / f64::from(rate)).sin()) as f32)
            .collect()
    }

    /// The hue of the last colour sent, from its RGB.
    fn last_hue(frames: &[Frame]) -> (f64, Colour) {
        let c = frames
            .iter()
            .rev()
            .find_map(|f| f.colour)
            .expect("a colour");
        let (r, g, b) = (f64::from(c.red), f64::from(c.green), f64::from(c.blue));
        let (max, min) = (r.max(g).max(b), r.min(g).min(b));
        let d = (max - min).max(1e-9);
        let h = if max == r {
            ((g - b) / d).rem_euclid(6.0)
        } else if max == g {
            (b - r) / d + 2.0
        } else {
            (r - g) / d + 4.0
        };
        (h * 60.0, c)
    }

    #[test]
    fn the_fft_of_a_bin_centred_cosine_is_one_bin() {
        let fft = Fft::new(64);
        let mut re: Vec<f64> = (0..64)
            .map(|i| (2.0 * PI * 5.0 * i as f64 / 64.0).cos())
            .collect();
        let mut im = vec![0.0; 64];
        fft.forward(&mut re, &mut im);
        for k in 0..64 {
            let m = (re[k] * re[k] + im[k] * im[k]).sqrt();
            let want = if k == 5 || k == 59 { 32.0 } else { 0.0 };
            assert!((m - want).abs() < 1e-9, "bin {} {}", k, m);
        }
    }

    #[test]
    fn the_bands_are_iec_sixth_octaves_from_21_hz_to_19_khz() {
        // IEC 61260-1 5.4.2 with b = 6: x = -34 gives 21.1 Hz, x = 25 18.8 kHz,
        // and the 1 kHz band of the third-octave set is the pair x = -1, 0
        // (centres 1000 G^(-1/12) and 1000 G^(1/12)).
        assert!((band_centre_hz(0) - 21.13).abs() < 0.01);
        assert!((band_centre_hz(BANDS - 1) - 18_836.5).abs() < 0.5);
        assert!((band_centre_hz(33) * band_centre_hz(34) - 1e6).abs() < 1e-3);
        let (lo, hi) = band_edges_hz(10);
        assert!((hi / lo - 10f64.powf(0.3 / 6.0)).abs() < 1e-12);
        assert!((band_edges_hz(10).1 - band_edges_hz(11).0).abs() < 1e-9);
    }

    #[test]
    fn hsv_primaries() {
        assert_eq!(hsv_to_rgb(0.0, 1.0, 1.0), (255, 0, 0));
        assert_eq!(hsv_to_rgb(120.0, 1.0, 1.0), (0, 255, 0));
        assert_eq!(hsv_to_rgb(240.0, 1.0, 1.0), (0, 0, 255));
        assert_eq!(hsv_to_rgb(60.0, 1.0, 1.0), (255, 255, 0));
        assert_eq!(hsv_to_rgb(200.0, 0.0, 1.0), (255, 255, 255));
    }

    #[test]
    fn pieces_and_channels_do_not_change_the_frames() {
        let x: Vec<f32> = (0..48_000)
            .map(|i| ((i * 7919) % 1000) as f32 / 1000.0 - 0.5)
            .collect();
        let mut a = Analyzer::new(48_000).unwrap();
        let whole = run(&mut a, &x);
        let mut b = Analyzer::new(48_000).unwrap();
        let mut pieces = Vec::new();
        let stereo: Vec<f32> = x.iter().flat_map(|s| [*s, *s]).collect();
        for c in stereo.chunks(734) {
            b.push(c, 2, &mut pieces);
        }
        assert_eq!(whole, pieces);
        assert!(whole.len() >= 20);
    }

    #[test]
    fn silence_is_silent_and_never_beats() {
        let mut a = Analyzer::new(48_000).unwrap();
        let frames = run(&mut a, &vec![0.0; 96_000]);
        assert!(frames.iter().all(|f| f.is_silent() && f.colour.is_none()));
    }

    #[test]
    fn low_tones_are_blue_high_tones_red_and_noise_unsaturated() {
        // The research's checks: a 100 Hz sine blue, an 8 kHz sine red,
        // white noise low in saturation.
        let mut a = Analyzer::new(48_000).unwrap();
        let (hue, c) = last_hue(&run(&mut a, &sine(48_000, 100.0, 0.5, 2.0)));
        assert!((hue - 240.0).abs() < 15.0 && c.blue == 255, "{hue} {c:?}");
        let mut a = Analyzer::new(48_000).unwrap();
        let (hue, c) = last_hue(&run(&mut a, &sine(48_000, 8_000.0, 0.5, 2.0)));
        assert!(hue < 15.0 || hue > 345.0, "{hue} {c:?}");
        let mut a = Analyzer::new(48_000).unwrap();
        let mut x = 1u32;
        let noise: Vec<f32> = (0..96_000)
            .map(|_| {
                x = x.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                (f64::from(x >> 8) / f64::from(1u32 << 24) - 0.5) as f32
            })
            .collect();
        let (_, c) = last_hue(&run(&mut a, &noise));
        let spread = c.red.max(c.green).max(c.blue) - c.red.min(c.green).min(c.blue);
        assert!(spread < 128, "noise is pale: {c:?}");
    }

    #[test]
    fn bands_merge_to_any_count() {
        let mut f = Frame {
            at_sample: 0,
            peak: 0,
            beat: 0,
            band_db: [FLOOR_DB as f32; BANDS],
            colour: None,
        };
        f.band_db[10] = -6.0;
        f.band_db[11] = -6.0;
        let mut out = Vec::new();
        f.bands(64, &mut out);
        assert_eq!(out.len(), BANDS, "at most the bands analysed");
        assert_eq!(out[10], level_byte(-6.0));
        f.bands(30, &mut out);
        assert_eq!(out.len(), 30);
        // Two sixth-octaves at -6 dB make a third-octave at -2.99 dB.
        assert_eq!(out[5], level_byte(-6.0 + 10.0 * 2f64.log10()));
        assert_eq!(out[4], 0);
        f.bands(0, &mut out);
        assert!(out.is_empty());
        f.bands(7, &mut out);
        assert_eq!(out.len(), 7);
    }
}
