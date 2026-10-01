//! The endpoint's chain: every block of this crate in one order, configured by
//! the room's [`SoundSettings`] and the endpoint's [`EndpointDsp`].
//!
//! Per frame, in this order (the C mirror's `chorus_dsp_chain_process` is the
//! same, step for step):
//!
//! 1. **Room EQ**: the room-correction peaking filters, on every channel but
//!    LFE, when `room_eq_enabled` (a 0 dB filter is skipped).
//! 2. **Tone**: a low shelf ([`BASS_HZ`]) by `bass_db` and a high shelf
//!    ([`TREBLE_HZ`]) by `treble_db`, every channel but LFE.
//! 3. **Loudness**: the ISO 226 shelves for the volume's attenuation
//!    ([`crate::loudness`]), every channel but LFE.
//! 4. **Speech**: the voice-band boost ([`crate::speech`]).
//! 5. **Night**: the channel-linked compressor ([`crate::compressor::NIGHT`]).
//! 6. **Bass management and the output's source**, by role: not in a set
//!    (role 0), the stream's channels as they are; a main role, that channel
//!    (the mono channel of a mono stream, silence if the stream lacks it),
//!    LR4 high-passed at `crossover_hz` when the set has a subwoofer; the
//!    `LFE` role, the LR4 low branch of the sum of the stream's main channels,
//!    plus the stream's LFE channel at +10 dB (ATSC A/52:2018, section 3: the
//!    LFE channel "is intended to be reproduced at a level +10 dB with respect
//!    to the fbw channels", <https://www.atsc.org/wp-content/uploads/2021/04/A52-2018.pdf>,
//!    read 2026-10-01; and "It is designed to be amplified by 10 dB on
//!    playback and summed into the signal going to the subwoofer",
//!    <https://en.wikipedia.org/wiki/Bass_management>, read 2026-10-01),
//!    times the subwoofer level, inverted if set. The theater maps (goal 13,
//!    `docs/dsp.md` "The theater maps") choose what a role plays when the
//!    stream and the set do not match: a front role of a set with no centre
//!    or no surrounds folds those in (ITU-R BS.775-4 Table 2), a centre role
//!    on a stream with no centre plays `(FL + FR) / sqrt 2`, a surround role
//!    takes the other surround pair's channel (side for back, back for side)
//!    or, on a stream with no surround at all, silence or the room's
//!    `tv_upmix` ambient feed, and an unbonded stereo speaker
//!    (`stereo_downmix`) or two-way takes the BS.775-4 downmix.
//! 7. **Two-way**: when the endpoint has one, its one input (the role's
//!    channel, or for role 0 the downmix `(FL + FR)/2`, the mono channel, or
//!    the mean of the main channels) split by LR4 into woofer (output 0) and
//!    tweeter (output 1), each times its trim, inverted if set.
//! 8. **Delay**: each output's delay (its `output_delay_us` plus, two-way, its
//!    driver's `delay_us`), whole frames.
//! 9. **Volume**: times the room gain.
//! 10. **Limiter**: the look-ahead limiter at `min(1, limit gain)` (K81, I10:
//!     no DSP boost lifts a room above its limit).
//!
//! A flat chain (default settings, role 0, no two-way, no delays) runs no
//! filter at all and its output is its input times the room gain, delayed by
//! the limiter's look-ahead, bit for bit. The latency is the look-ahead
//! whatever the settings, so changing a setting never moves the audio in
//! time. [`Chain::set_sound`] keeps every running filter's state when only
//! gains change, so it can be called on every `sound` message.

use crate::biquad::{Biquad, Coefficients, Kind};
use crate::compressor::{Compressor, NIGHT};
use crate::crossover::{Lr4, Lr4Design};
use crate::delay::{self, Delay};
use crate::limiter::{self, Limiter};
use crate::loudness;
use crate::settings::{
    position, EndpointDsp, SoundSettings, MAX_OUTPUTS, ROOM_EQ_MAX_FILTERS, TV_UPMIX_AMBIENT,
};
use crate::speech::{self, Mode};
use crate::{db_to_gain, DspError, MAX_RATE_HZ, MIN_RATE_HZ};

/// The bass shelf's midpoint. ASSUMED: 100 Hz.
pub const BASS_HZ: f64 = 100.0;
/// The treble shelf's midpoint. ASSUMED: 8 kHz.
pub const TREBLE_HZ: f64 = 8000.0;
/// Both tone shelves' Q. ASSUMED: 1/sqrt 2.
pub const TONE_Q: f64 = core::f64::consts::FRAC_1_SQRT_2;
/// The LFE channel's gain into the subwoofer feed, dB (cited above).
pub const LFE_GAIN_DB: f64 = 10.0;
/// 1/sqrt 2, every downmix and fold coefficient: ITU-R BS.775-4 (12/2022),
/// Annex 4 Table 2 prints it as 0.7071 (for example the 2/0 format's
/// `L = 1.0000 L + 0.7071 C + 0.7071 LS`),
/// <https://www.itu.int/dms_pubrec/itu-r/rec/bs/R-REC-BS.775-4-202212-I!!PDF-E.pdf>,
/// read 2026-10-01. The exact value is used; the table's is it rounded.
pub const DOWNMIX_GAIN: f64 = core::f64::consts::FRAC_1_SQRT_2;
/// The ambient surround's gain on `FL - FR`: the passive matrix's. Its
/// encoder puts the surround into Lt and Rt "divided equally" at -3 dB with
/// opposite signs (R. Dressler, "Dolby Surround Pro Logic Decoder Principles
/// of Operation", section 1.1,
/// <https://educypedia.org/library/208_Dolby_Surround_Pro_Logic_Decoder.pdf>,
/// read 2026-10-01), so `(Lt - Rt) / sqrt 2` gives it back at unity
/// (computed). The same reasoning gives the centre's `(Lt + Rt) / sqrt 2`.
pub const AMBIENT_GAIN: f64 = core::f64::consts::FRAC_1_SQRT_2;
/// The ambient surround's delay, us: "the crosstalk from the surrounds will
/// arrive at the listener about 20mS after the direct sound from the front
/// channels" (Sound On Sound, "Surround Sound Explained: Part 2",
/// <https://www.soundonsound.com/techniques/surround-sound-explained-part-2>,
/// read 2026-10-01; Dressler section 2: the delay uses the precedence effect
/// so leakage does not pull the image off the screen).
pub const AMBIENT_DELAY_US: u32 = 20_000;
/// The ambient surround's band: "a band-pass filter to remove frequencies
/// below 100Hz and above 7kHz" (Sound On Sound, above; Dressler section 1.2:
/// "a 7 kHz low-pass filter").
pub const AMBIENT_LOW_HZ: f64 = 100.0;
/// See [`AMBIENT_LOW_HZ`].
pub const AMBIENT_HIGH_HZ: f64 = 7000.0;
/// The band's two sections' Q (one RBJ high-pass, one RBJ low-pass). ASSUMED:
/// 1/sqrt 2 (Butterworth); the sources give the corners, not the slopes.
pub const AMBIENT_Q: f64 = core::f64::consts::FRAC_1_SQRT_2;
/// The most terms one mix row holds: the seven main channels of a 7.1 map.
pub const MIX_TERMS: usize = 7;

/// The delay frames all of a chain's outputs may hold between them: two
/// outputs at [`delay::MAX_FRAMES`] (so the C chain's fixed pool is 38.4 KB).
pub const DELAY_POOL_FRAMES: usize = 2 * delay::MAX_FRAMES;

/// Every corner frequency is held to at most 0.45 x the rate, so a design
/// never meets Nyquist at a low stream rate. ASSUMED margin.
pub fn corner(freq_hz: f64, rate_hz: f64) -> f64 {
    let top = 0.45 * rate_hz;
    if freq_hz > top {
        top
    } else {
        freq_hz
    }
}

/// One output's weighted sum of stream channels, summed in the order the
/// terms were added (the C mirror adds them in the same order, so the two
/// agree bit for bit). Each gain is designed in `f64` and rounded to `f32`
/// once.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Mix {
    at: [usize; MIX_TERMS],
    gain: [f32; MIX_TERMS],
    n: usize,
}

impl Mix {
    /// A row of `terms` (position, gain), each present in `map` taken in the
    /// order given; a position the map lacks is skipped.
    fn row(map: &[u8], terms: &[(u8, f64)]) -> Mix {
        let mut m = Mix {
            at: [0; MIX_TERMS],
            gain: [0.0; MIX_TERMS],
            n: 0,
        };
        for &(p, g) in terms {
            if let Some(i) = map.iter().position(|&q| q == p) {
                if m.n < MIX_TERMS {
                    m.at[m.n] = i;
                    m.gain[m.n] = g as f32;
                    m.n += 1;
                }
            }
        }
        m
    }

    fn apply(&self, x: &[f32]) -> f32 {
        let mut v = 0.0f32;
        for k in 0..self.n {
            v += x[self.at[k]] * self.gain[k];
        }
        v
    }
}

/// The surround positions on a side: left (`BL`, `SL`) or right.
fn side(left: bool) -> [u8; 2] {
    if left {
        [position::BL, position::SL]
    } else {
        [position::BR, position::SR]
    }
}

fn is_surround(p: u8) -> bool {
    matches!(p, position::BL | position::BR | position::SL | position::SR)
}

/// The other 5.1 surround naming of the same speaker: a 5.1 stream carries
/// its surrounds as back (BL BR, `KSAUDIO_SPEAKER_5POINT1`) or side (SL SR,
/// `KSAUDIO_SPEAKER_5POINT1_SURROUND`, which Microsoft names "the speaker
/// configuration for a 5.1-channel surround format"),
/// <https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/ksmedia/ns-ksmedia-ksaudio_channel_config>,
/// read 2026-10-01.
fn equivalent(p: u8) -> u8 {
    match p {
        position::BL => position::SL,
        position::SL => position::BL,
        position::BR => position::SR,
        position::SR => position::BR,
        other => other,
    }
}

/// The BS.775-4 2/0 row for one side: the front channel, the centre and the
/// side's surround(s), each surround at 1/sqrt 2.
fn stereo_row(left: bool) -> [(u8, f64); 4] {
    let g = DOWNMIX_GAIN;
    let s = side(left);
    let front = if left { position::FL } else { position::FR };
    [(front, 1.0), (position::FC, g), (s[0], g), (s[1], g)]
}

/// The mono row a two-way endpoint takes from a stream with a centre or
/// surrounds: the mean of BS.775-4's 2/0 pair, `(Lo + Ro) / 2`, the same
/// "mean of the stereo pair" rule the chain already applies to a stereo
/// stream (so a 5.1 and a 2.0 mix of one programme play at one level; the
/// table's 1/0 row would play the 5.1 3 dB hotter).
fn mono_row() -> [(u8, f64); 7] {
    let g = DOWNMIX_GAIN;
    [
        (position::FL, 0.5),
        (position::FR, 0.5),
        (position::FC, g),
        (position::BL, 0.5 * g),
        (position::BR, 0.5 * g),
        (position::SL, 0.5 * g),
        (position::SR, 0.5 * g),
    ]
}

/// Where the (first) output's signal comes from, before the two-way split.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Source {
    /// Every stream channel to its own output.
    Pass,
    /// One stream channel; `None` is silence.
    Channel(Option<usize>),
    /// The subwoofer feed.
    Sub,
    /// The mean of two channels.
    Pair(usize, usize),
    /// The mean of the main channels.
    MeanOfMains,
    /// One weighted sum (a fold, the centre upmix, a two-way's downmix).
    Mix(Mix),
    /// Two outputs, the BS.775-4 2/0 downmix (`stereo_downmix`).
    Downmix(Mix, Mix),
    /// The ambient surround from these two channels (FL, FR).
    Ambient(usize, usize),
}

/// The running chain. See the module for the order.
#[derive(Clone, Debug)]
pub struct Chain {
    rate: u32,
    map: Vec<u8>,
    endpoint: EndpointDsp,
    settings: SoundSettings,
    eq: Vec<[Biquad; ROOM_EQ_MAX_FILTERS]>,
    eq_count: usize,
    tone: Vec<[Biquad; 2]>,
    tone_on: [bool; 2],
    loud: Vec<[Biquad; 2]>,
    loud_on: [bool; 2],
    loud_att: f64,
    speech: Biquad,
    speech_mode: Mode,
    night: Compressor,
    source: Source,
    highpass: bool,
    bass: Lr4,
    ambient: [Biquad; 2],
    sub_gain: f32,
    lfe_gain: f32,
    ambient_gain: f32,
    lfe: Option<usize>,
    two_way: Option<(Lr4, [f32; 2])>,
    delays: Vec<Delay>,
    limiter: Limiter,
    frame: Vec<f32>,
    out: Vec<f32>,
}

fn is_main(p: u8) -> bool {
    p != position::LFE
}

fn validate_map(map: &[u8]) -> Result<(), DspError> {
    if map.is_empty() || map.len() > 8 {
        return Err(DspError::ChannelMap);
    }
    for (i, &p) in map.iter().enumerate() {
        if p > position::MAX || map[..i].contains(&p) {
            return Err(DspError::ChannelMap);
        }
        if p == position::MONO && map.len() != 1 {
            return Err(DspError::ChannelMap);
        }
    }
    Ok(())
}

impl Chain {
    /// A chain for a stream of `map` (one position per interleaved channel)
    /// at `rate_hz`, with the room's `settings` and the endpoint's own
    /// `endpoint` configuration.
    pub fn new(
        settings: &SoundSettings,
        endpoint: &EndpointDsp,
        map: &[u8],
        rate_hz: u32,
    ) -> Result<Chain, DspError> {
        if !(MIN_RATE_HZ..=MAX_RATE_HZ).contains(&rate_hz) {
            return Err(DspError::Rate);
        }
        validate_map(map)?;
        endpoint.validate(rate_hz)?;
        settings.validate()?;
        let n = map.len();
        let rate = rate_hz as f64;
        let lookahead = delay::frames_for_us(limiter::DEFAULT_LOOKAHEAD_US, rate_hz) as usize;
        let mut chain = Chain {
            rate: rate_hz,
            map: map.to_vec(),
            endpoint: *endpoint,
            settings: SoundSettings::default(),
            eq: vec![[Biquad::default(); ROOM_EQ_MAX_FILTERS]; n],
            eq_count: 0,
            tone: vec![[Biquad::default(); 2]; n],
            tone_on: [false; 2],
            loud: vec![[Biquad::default(); 2]; n],
            loud_on: [false; 2],
            loud_att: 0.0,
            speech: Biquad::new(&speech::design(rate)?),
            speech_mode: speech::mode_for(map),
            night: Compressor::new(&NIGHT, rate)?,
            source: Source::Pass,
            highpass: false,
            bass: Lr4::default(),
            ambient: [Biquad::default(); 2],
            sub_gain: 1.0,
            lfe_gain: db_to_gain(LFE_GAIN_DB) as f32,
            ambient_gain: AMBIENT_GAIN as f32,
            lfe: map.iter().position(|&p| p == position::LFE),
            two_way: None,
            delays: Vec::new(),
            limiter: Limiter::new(0, lookahead, limiter::DEFAULT_RELEASE_MS, rate)?,
            frame: vec![0.0; n],
            out: Vec::new(),
        };
        if let Some(t) = &endpoint.two_way {
            let d = Lr4Design::new(rate, t.crossover_hz as f64)?;
            let trim = |d: &crate::settings::Driver| {
                let g = db_to_gain(d.trim_cdb as f64 / 100.0) as f32;
                if d.inverted {
                    -g
                } else {
                    g
                }
            };
            chain.two_way = Some((Lr4::new(&d), [trim(&t.woofer), trim(&t.tweeter)]));
        }
        chain.apply(settings, true)?;
        Ok(chain)
    }

    /// The stream's channel count.
    pub fn in_channels(&self) -> usize {
        self.map.len()
    }

    /// The chain's output count: the stream's channels when not in a set and
    /// not two-way, 2 when two-way, 1 otherwise.
    pub fn out_channels(&self) -> usize {
        self.out.len()
    }

    /// The latency every output carries, frames: the limiter's look-ahead.
    pub fn latency_frames(&self) -> usize {
        self.limiter.lookahead()
    }

    /// The sample rate the chain was built for.
    pub fn rate_hz(&self) -> u32 {
        self.rate
    }

    /// The settings in force.
    pub fn settings(&self) -> &SoundSettings {
        &self.settings
    }

    /// Applies new room settings. Every running filter keeps its state when
    /// only its gain changes; a stage that switches on starts from zero
    /// state; a change of role, subwoofer presence or crossover that changes
    /// the outputs resets the output side (bass management, two-way, delays,
    /// limiter). Refused settings change nothing.
    pub fn set_sound(&mut self, settings: &SoundSettings) -> Result<(), DspError> {
        settings.validate()?;
        self.apply(settings, false)
    }

    fn apply(&mut self, s: &SoundSettings, first: bool) -> Result<(), DspError> {
        let rate = self.rate as f64;
        let n = self.map.len();

        // 1. Room EQ: the non-zero filters, in order.
        let mut designs = [Coefficients::IDENTITY; ROOM_EQ_MAX_FILTERS];
        let mut count = 0;
        if s.room_eq_enabled {
            for f in &s.room_eq {
                if f.gain_cdb == 0 {
                    continue;
                }
                designs[count] = Coefficients::design(
                    Kind::Peaking,
                    rate,
                    corner(f.freq_hz as f64, rate),
                    f.q_milli as f64 / 1000.0,
                    f.gain_cdb as f64 / 100.0,
                )?;
                count += 1;
            }
        }
        // 2. Tone.
        let bass = Coefficients::design(
            Kind::LowShelf,
            rate,
            corner(BASS_HZ, rate),
            TONE_Q,
            s.bass_db as f64,
        )?;
        let treble = Coefficients::design(
            Kind::HighShelf,
            rate,
            corner(TREBLE_HZ, rate),
            TONE_Q,
            s.treble_db as f64,
        )?;
        let tone_on = [s.bass_db != 0, s.treble_db != 0];

        // 6. The output's source.
        let role = s.role;
        let map = &self.map;
        let find = |p: u8| map.iter().position(|&q| q == p);
        // A centre or a surround beside the front pair: what a downmix folds.
        let extra = map.iter().any(|&p| p == position::FC || is_surround(p));
        let front = (find(position::FL), find(position::FR));
        let (source, highpass) = if role == 0 {
            if self.two_way.is_some() {
                let src = match front {
                    (Some(_), Some(_)) if extra => Source::Mix(Mix::row(map, &mono_row())),
                    (Some(l), Some(r)) => Source::Pair(l, r),
                    _ if n == 1 => Source::Channel(Some(0)),
                    _ => Source::MeanOfMains,
                };
                (src, false)
            } else if self.endpoint.stereo_downmix
                && extra
                && front.0.is_some()
                && front.1.is_some()
            {
                let lo = Mix::row(map, &stereo_row(true));
                let ro = Mix::row(map, &stereo_row(false));
                (Source::Downmix(lo, ro), false)
            } else {
                (Source::Pass, false)
            }
        } else if role == position::LFE {
            (Source::Sub, false)
        } else if let Some(i) = find(role) {
            // The role's own channel, and for a front role the folds the
            // set asks for (BS.775-4 Table 2: 3/0 folds the surrounds, 2/2
            // the centre, 2/0 both).
            let mut terms = vec![(role, 1.0)];
            if role == position::FL || role == position::FR {
                if s.fold_centre {
                    terms.push((position::FC, DOWNMIX_GAIN));
                }
                if s.fold_surround {
                    for p in side(role == position::FL) {
                        terms.push((p, DOWNMIX_GAIN));
                    }
                }
            }
            let m = Mix::row(map, &terms);
            let src = if m.n == 1 {
                Source::Channel(Some(i))
            } else {
                Source::Mix(m)
            };
            (src, s.sub_present)
        } else if self.map == [position::MONO] {
            (Source::Channel(Some(0)), s.sub_present)
        } else if role == position::FC {
            // The passive centre, (FL + FR) / sqrt 2; silence without both.
            let src = match front {
                (Some(_), Some(_)) => Source::Mix(Mix::row(
                    map,
                    &[(position::FL, DOWNMIX_GAIN), (position::FR, DOWNMIX_GAIN)],
                )),
                _ => Source::Channel(None),
            };
            (src, s.sub_present)
        } else if is_surround(role) {
            let any_surround = map.iter().any(|&p| is_surround(p));
            let src = match (find(equivalent(role)), front) {
                (Some(k), _) => Source::Channel(Some(k)),
                (None, (Some(l), Some(r))) if !any_surround && s.tv_upmix == TV_UPMIX_AMBIENT => {
                    Source::Ambient(l, r)
                }
                _ => Source::Channel(None),
            };
            (src, s.sub_present)
        } else {
            (Source::Channel(None), s.sub_present)
        };
        let ambient = matches!(source, Source::Ambient(..));
        let base_outputs = match source {
            Source::Pass => n,
            Source::Downmix(..) => 2,
            _ => 1,
        };
        let outputs = if self.two_way.is_some() {
            2
        } else {
            base_outputs
        };
        // 8. Delays: refused before anything changes.
        let mut frames = [0usize; MAX_OUTPUTS];
        let mut pool = 0usize;
        for (o, slot) in frames.iter_mut().enumerate().take(outputs) {
            let mut us = self.endpoint.output_delay_us[o] as u64;
            if ambient {
                us += AMBIENT_DELAY_US as u64;
            }
            if let Some(t) = &self.endpoint.two_way {
                us += if o == 0 {
                    t.woofer.delay_us
                } else {
                    t.tweeter.delay_us
                } as u64;
            }
            let us = u32::try_from(us).map_err(|_| DspError::DelayTooLong)?;
            let f = delay::frames_for_us(us, self.rate);
            if f > delay::MAX_FRAMES as u64 {
                return Err(DspError::DelayTooLong);
            }
            *slot = f as usize;
            pool += f as usize;
        }
        if pool > DELAY_POOL_FRAMES {
            return Err(DspError::DelayTooLong);
        }
        let bass_design = Lr4Design::new(rate, corner(s.crossover_hz as f64, rate))?;
        let ambient_designs = [
            Coefficients::design(
                Kind::Highpass,
                rate,
                corner(AMBIENT_LOW_HZ, rate),
                AMBIENT_Q,
                0.0,
            )?,
            Coefficients::design(
                Kind::Lowpass,
                rate,
                corner(AMBIENT_HIGH_HZ, rate),
                AMBIENT_Q,
                0.0,
            )?,
        ];

        // Commit.
        for c in 0..n {
            for (k, d) in designs.iter().enumerate().take(count) {
                if k >= self.eq_count {
                    self.eq[c][k].reset();
                }
                self.eq[c][k].set(d);
            }
            for (k, d) in [bass, treble].iter().enumerate() {
                if tone_on[k] && !self.tone_on[k] {
                    self.tone[c][k].reset();
                }
                self.tone[c][k].set(d);
            }
        }
        self.eq_count = count;
        self.tone_on = tone_on;
        if !s.loudness {
            self.loud_on = [false; 2];
            self.loud_att = 0.0;
        }
        if s.night && !self.settings.night {
            self.night.reset();
        }
        if s.speech && !self.settings.speech {
            self.speech.reset();
        }
        let layout_changed = first
            || source != self.source
            || highpass != self.highpass
            || outputs != self.out.len()
            || s.crossover_hz != self.settings.crossover_hz
            || frames
                .iter()
                .take(outputs)
                .zip(self.delays.iter())
                .any(|(&f, d)| f != d.frames());
        self.bass.set(&bass_design);
        let sub = db_to_gain(s.sub_level_cdb as f64 / 100.0) as f32;
        self.sub_gain = if s.sub_polarity_inverted { -sub } else { sub };
        if layout_changed {
            self.source = source;
            self.highpass = highpass;
            self.bass.reset();
            for (b, d) in self.ambient.iter_mut().zip(ambient_designs.iter()) {
                *b = Biquad::new(d);
            }
            if let Some((split, _)) = &mut self.two_way {
                split.reset();
            }
            self.delays = frames
                .iter()
                .take(outputs)
                .map(|&f| Delay::new(f))
                .collect::<Result<_, _>>()?;
            self.limiter = Limiter::new(
                outputs,
                self.limiter.lookahead(),
                limiter::DEFAULT_RELEASE_MS,
                rate,
            )?;
            self.out = vec![0.0; outputs];
        }
        self.settings = s.clone();
        Ok(())
    }

    /// Redesigns the loudness shelves when the quantised attenuation moved.
    fn update_loudness(&mut self, room_gain: f32) -> Result<(), DspError> {
        if !self.settings.loudness {
            return Ok(());
        }
        let att = loudness::attenuation_db(room_gain);
        if att == self.loud_att {
            return Ok(());
        }
        let rate = self.rate as f64;
        let (low, high) = loudness::shelf_gains_db(att);
        let designs = [
            Coefficients::design(
                Kind::LowShelf,
                rate,
                corner(loudness::LOW_SHELF_HZ, rate),
                loudness::SHELF_Q,
                low,
            )?,
            Coefficients::design(
                Kind::HighShelf,
                rate,
                corner(loudness::HIGH_SHELF_HZ, rate),
                loudness::SHELF_Q,
                high,
            )?,
        ];
        let on = [low != 0.0, high != 0.0];
        for stages in &mut self.loud {
            for k in 0..2 {
                if on[k] && !self.loud_on[k] {
                    stages[k].reset();
                }
                stages[k].set(&designs[k]);
            }
        }
        self.loud_on = on;
        self.loud_att = att;
        Ok(())
    }

    /// Processes interleaved frames: `input` holds whole frames of
    /// [`Chain::in_channels`], `output` exactly as many frames of
    /// [`Chain::out_channels`]. `room_gain` is the room's linear gain (from
    /// `room_volume`), `limit_gain` the room's effective limit (linear); the
    /// limiter's ceiling is `min(1, limit_gain)`. Returns the frame count.
    // The channel loops index by channel on purpose: they are written the way
    // firmware/src/dsp.c writes them, so the two read side by side.
    #[allow(clippy::needless_range_loop)]
    pub fn process(
        &mut self,
        input: &[f32],
        output: &mut [f32],
        room_gain: f32,
        limit_gain: f32,
    ) -> Result<usize, DspError> {
        let n = self.map.len();
        let outs = self.out.len();
        if !input.len().is_multiple_of(n) || output.len() != input.len() / n * outs {
            return Err(DspError::Buffer);
        }
        self.update_loudness(room_gain)?;
        self.limiter
            .set_ceiling(if limit_gain < 1.0 { limit_gain } else { 1.0 });
        let frames = input.len() / n;
        let lfe = self.lfe;
        for f in 0..frames {
            self.frame.copy_from_slice(&input[f * n..(f + 1) * n]);
            let x = &mut self.frame;
            // 1-3. Room EQ, tone, loudness, every channel but LFE.
            for c in 0..n {
                if Some(c) == lfe {
                    continue;
                }
                let mut v = x[c];
                for k in 0..self.eq_count {
                    v = self.eq[c][k].process(v);
                }
                for k in 0..2 {
                    if self.tone_on[k] {
                        v = self.tone[c][k].process(v);
                    }
                }
                for k in 0..2 {
                    if self.loud_on[k] {
                        v = self.loud[c][k].process(v);
                    }
                }
                x[c] = v;
            }
            // 4. Speech.
            if self.settings.speech {
                match self.speech_mode {
                    Mode::Channel(i) => x[i] = self.speech.process(x[i]),
                    Mode::MidSide(l, r) => {
                        let m = (x[l] + x[r]) * 0.5;
                        let s = (x[l] - x[r]) * 0.5;
                        let m = self.speech.process(m);
                        x[l] = m + s;
                        x[r] = m - s;
                    }
                    Mode::None => {}
                }
            }
            // 5. Night.
            if self.settings.night {
                self.night.process_frame(x);
            }
            // 6. Bass management and the source.
            match self.source {
                Source::Pass => self.out.copy_from_slice(x),
                Source::Channel(at) => {
                    let mut v = match at {
                        Some(i) => x[i],
                        None => 0.0,
                    };
                    if self.highpass {
                        v = self.bass.high(v);
                    }
                    self.out[0] = v;
                }
                Source::Sub => {
                    let mut sum = 0.0f32;
                    for c in 0..n {
                        if is_main(self.map[c]) {
                            sum += x[c];
                        }
                    }
                    let mut v = self.bass.low(sum);
                    if let Some(i) = lfe {
                        v += x[i] * self.lfe_gain;
                    }
                    self.out[0] = v * self.sub_gain;
                }
                Source::Pair(l, r) => self.out[0] = (x[l] + x[r]) * 0.5,
                Source::Mix(m) => {
                    let mut v = m.apply(x);
                    if self.highpass {
                        v = self.bass.high(v);
                    }
                    self.out[0] = v;
                }
                Source::Downmix(lo, ro) => {
                    self.out[0] = lo.apply(x);
                    self.out[1] = ro.apply(x);
                }
                Source::Ambient(l, r) => {
                    let mut v = (x[l] - x[r]) * self.ambient_gain;
                    v = self.ambient[0].process(v);
                    v = self.ambient[1].process(v);
                    if self.highpass {
                        v = self.bass.high(v);
                    }
                    self.out[0] = v;
                }
                Source::MeanOfMains => {
                    let mut sum = 0.0f32;
                    let mut count = 0u32;
                    for c in 0..n {
                        if is_main(self.map[c]) {
                            sum += x[c];
                            count += 1;
                        }
                    }
                    self.out[0] = if count > 0 {
                        sum * (1.0 / count as f32)
                    } else {
                        0.0
                    };
                }
            }
            // 7. Two-way.
            if let Some((split, trims)) = &mut self.two_way {
                let (lo, hi) = split.split(self.out[0]);
                self.out[0] = lo * trims[0];
                self.out[1] = hi * trims[1];
            }
            // 8-9. Delay, volume.
            for (o, d) in self.out.iter_mut().zip(self.delays.iter_mut()) {
                *o = d.process(*o) * room_gain;
            }
            // 10. Limiter.
            self.limiter.process_frame(&mut self.out);
            output[f * outs..(f + 1) * outs].copy_from_slice(&self.out);
        }
        Ok(frames)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::{Driver, TwoWay};

    #[test]
    fn a_flat_chain_is_the_input_times_gain_delayed() {
        let mut c = Chain::new(
            &SoundSettings::default(),
            &EndpointDsp::default(),
            &[1, 2],
            48000,
        )
        .unwrap();
        assert_eq!(c.out_channels(), 2);
        let lat = c.latency_frames();
        assert_eq!(lat, 96);
        let input: Vec<f32> = (0..2000)
            .map(|i| ((i as f32) * 0.013).sin() * 0.9)
            .collect();
        let mut out = vec![0.0; 2000];
        c.process(&input, &mut out, 0.7, 1.0).unwrap();
        for i in 2 * lat..2000 {
            assert_eq!(out[i].to_bits(), (input[i - 2 * lat] * 0.7).to_bits());
        }
    }

    #[test]
    fn refusals_change_nothing() {
        let mut c = Chain::new(
            &SoundSettings::default(),
            &EndpointDsp::default(),
            &[1, 2],
            48000,
        )
        .unwrap();
        let bad = SoundSettings {
            bass_db: 11,
            ..SoundSettings::default()
        };
        assert_eq!(c.set_sound(&bad), Err(DspError::Setting("bass_db")));
        assert_eq!(c.settings(), &SoundSettings::default());
        assert_eq!(
            Chain::new(
                &SoundSettings::default(),
                &EndpointDsp::default(),
                &[1, 1],
                48000
            )
            .err(),
            Some(DspError::ChannelMap)
        );
        let long = EndpointDsp {
            output_delay_us: [60_000, 0, 0, 0, 0, 0, 0, 0],
            ..EndpointDsp::default()
        };
        assert_eq!(
            Chain::new(&SoundSettings::default(), &long, &[1, 2], 96000).err(),
            Some(DspError::DelayTooLong)
        );
    }

    #[test]
    fn the_outputs_follow_role_and_two_way() {
        let two = EndpointDsp {
            two_way: Some(TwoWay {
                crossover_hz: 2000,
                woofer: Driver::default(),
                tweeter: Driver::default(),
            }),
            ..EndpointDsp::default()
        };
        let main = SoundSettings {
            role: 1,
            sub_present: true,
            ..SoundSettings::default()
        };
        let c = Chain::new(&main, &EndpointDsp::default(), &[1, 2, 4], 48000).unwrap();
        assert_eq!(c.out_channels(), 1);
        let c = Chain::new(&SoundSettings::default(), &two, &[1, 2], 48000).unwrap();
        assert_eq!(c.out_channels(), 2);
        let c = Chain::new(
            &SoundSettings::default(),
            &EndpointDsp::default(),
            &[1, 2, 3, 4, 5, 6],
            48000,
        )
        .unwrap();
        assert_eq!(c.out_channels(), 6);
    }

    #[test]
    fn the_theater_maps_pick_their_outputs() {
        let five = [1, 2, 3, 4, 5, 6];
        // tv_upmix past the last defined value is refused by name.
        let bad = SoundSettings {
            tv_upmix: 2,
            ..SoundSettings::default()
        };
        assert_eq!(
            Chain::new(&bad, &EndpointDsp::default(), &[1, 2], 48000).err(),
            Some(DspError::Setting("tv_upmix"))
        );
        // A stereo speaker downmixes 5.1 to two outputs and passes stereo.
        let stereo = EndpointDsp {
            stereo_downmix: true,
            ..EndpointDsp::default()
        };
        let s = SoundSettings::default();
        assert_eq!(
            Chain::new(&s, &stereo, &five, 48000)
                .unwrap()
                .out_channels(),
            2
        );
        assert_eq!(
            Chain::new(&s, &stereo, &[1, 2], 48000)
                .unwrap()
                .out_channels(),
            2
        );
        // The ambient surround's 20 ms rides the output's delay line, so a
        // rate where it does not fit is refused, not silently shortened.
        let amb = SoundSettings {
            role: position::SL,
            tv_upmix: TV_UPMIX_AMBIENT,
            ..SoundSettings::default()
        };
        assert!(Chain::new(&amb, &EndpointDsp::default(), &[1, 2], 192_000).is_ok());
        assert_eq!(
            Chain::new(&amb, &EndpointDsp::default(), &[1, 2], 384_000).err(),
            Some(DspError::DelayTooLong)
        );
        // A role change between upmix and plain channel re-plans the output.
        let mut c = Chain::new(&amb, &EndpointDsp::default(), &[1, 2], 48000).unwrap();
        let off = SoundSettings {
            tv_upmix: 0,
            ..amb.clone()
        };
        c.set_sound(&off).unwrap();
        assert_eq!(c.settings().tv_upmix, 0);
    }
}
