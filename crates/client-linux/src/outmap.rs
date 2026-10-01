//! The output map: N device channels, each fed from the stream by position,
//! with its own gain and its own delay.
//!
//! # What it is for
//!
//! A Linux endpoint with more outputs than the stream has channels, or with
//! outputs in a different order: the rack amp's zones (K74), where one card
//! carries several stereo pairs and a sub or line out, and the theater hub
//! (K72), which takes a stereo stream today and a 5.1 stream later. The
//! stream says what each of its channels IS (`stream_format.channel_map`,
//! `docs/protocol.md` "The channel map"); this map says which of those each
//! device channel plays. It is the remap `docs/protocol.md` puts "once, where
//! it meets" a transport, and ALSA is the transport it meets here.
//!
//! # Where it runs, and what it leaves alone
//!
//! At the last edge before the device: [`MappedSink`] wraps the sink the
//! client plays into, so the remap happens inside `write`, after the zone gain
//! (`crate::zone`) has already been applied to the stream's frames. Frames in
//! are frames out, one for one: the buffer, the sync loop, the playout
//! corrector and the delay the device reports are all counted in the same
//! frames with or without a map, and nothing upstream can tell it is there.
//!
//! A channel with a delay of `k` frames plays each sample `k` frames after the
//! frame it came in with. Its sound is therefore LATER THAN THE SYNC TARGET BY
//! EXACTLY ITS DELAY, on purpose: that is what a delay is for (a speaker
//! nearer the listener than the others, a sub whose path is shorter). The
//! first `k` frames of a delayed channel are silence, and the last `k` frames
//! it was given are still in its delay line when the session ends and are not
//! played, because flushing them would write frames the stream never had.
//!
//! # Exact where it can be
//!
//! An output fed from one position at 0 dB is a byte copy of that channel's
//! samples, so a stream through a map that only reorders is bit-exact in all
//! three formats. Anything else (a gain, a downmix) is computed in `f64`,
//! rounded to the nearest integer sample for the integer formats and
//! SATURATED at full scale, never wrapped; a float output is clamped to
//! [-1.0, 1.0]. Every saturated sample is counted ([`MappedSink::clipped_samples`]).
//!
//! # Nothing missing is guessed
//!
//! A position the map reads and the stream does not carry is silence on that
//! output, and [`ResolvedMap::report`] says so on a line of its own. The one
//! fallback is a mono stream (`MONO`): an output that reads `FL`, `FR` or `FC`
//! plays the mono channel, which is what a mono source on a stereo pair is
//! expected to do, and that is reported too.

use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use chorus_protocol::v2::ChannelPosition;
use chorus_protocol::{SampleFormat, MAX_CHANNELS};

use crate::sink::{PcmSink, SinkError, SinkWrite};

/// The most device channels a map drives: the protocol's own channel bound
/// (`capabilities.max_channels` is 1 to 8, `docs/protocol.md`).
pub const MAX_OUTPUT_CHANNELS: u16 = MAX_CHANNELS;

/// The loudest an output may be set, in dB relative to its source.
///
/// ASSUMED (not measured, chosen): +6 dB is enough to undo the 6.02 dB an
/// equal-weight two-position downmix costs (a sub fed from `FL+FR`), and no
/// more. A larger boost is a volume stage, and volume belongs to the zone
/// (the control catalog's gain, which the server limits, brief section 4.8,
/// I10) and to the amplifier, not to a static per-channel trim. Whatever the
/// boost, an output never exceeds digital full scale: it saturates.
pub const MAX_GAIN_DB: f64 = 6.0;

/// The quietest an output may be set, in dB, short of `silence`.
///
/// ASSUMED (chosen): -60 dB leaves a 16-bit full-scale sine about 33 units
/// peak, a trim nobody sets on purpose; an output meant to be off is
/// `silence`, which says so instead of being very quiet.
pub const MIN_GAIN_DB: f64 = -60.0;

/// The longest delay an output may be given, in microseconds.
///
/// ASSUMED (chosen): 50 ms is 17 m of path at 343 m/s, which covers any
/// speaker-distance or sub-alignment trim inside one room, and is well under
/// the default minimum buffer bound the client runs with (60 ms, `config.rs`), so a
/// delayed channel can never be further behind than the buffer the whole
/// endpoint is graded against. At the highest rate the protocol carries
/// (384 kHz) it is 19200 frames, 77 KB a channel in `f32`.
pub const MAX_DELAY_US: u64 = 50_000;

/// Where one device channel's samples come from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutputSource {
    /// One stream position, as it is.
    Position(ChannelPosition),
    /// Two or more positions summed with equal weight (each `1/n`), so a
    /// downmix of full-scale correlated channels is itself at most full scale.
    Downmix(Vec<ChannelPosition>),
    /// Nothing: this output is written zeros.
    Silence,
}

impl OutputSource {
    /// The positions this source reads.
    pub fn positions(&self) -> &[ChannelPosition] {
        match self {
            OutputSource::Position(p) => std::slice::from_ref(p),
            OutputSource::Downmix(ps) => ps,
            OutputSource::Silence => &[],
        }
    }

    /// The source as it is written on a command line.
    pub fn literal(&self) -> String {
        match self {
            OutputSource::Silence => "silence".to_string(),
            other => other
                .positions()
                .iter()
                .map(|p| p.name())
                .collect::<Vec<_>>()
                .join("+"),
        }
    }
}

/// One device channel's source, gain and delay, as configured.
#[derive(Debug, Clone, PartialEq)]
pub struct OutputSpec {
    /// Where its samples come from.
    pub source: OutputSource,
    /// Its gain in dB, [`MIN_GAIN_DB`] to [`MAX_GAIN_DB`].
    pub gain_db: f64,
    /// Its delay in microseconds, 0 to [`MAX_DELAY_US`]; applied as whole
    /// frames at the stream's rate ([`delay_frames`]).
    pub delay_us: u64,
}

impl OutputSpec {
    /// Silence, at 0 dB, undelayed: what an output nobody listed plays.
    pub fn silence() -> OutputSpec {
        OutputSpec {
            source: OutputSource::Silence,
            gain_db: 0.0,
            delay_us: 0,
        }
    }
}

/// The delay in whole frames that `delay_us` is at `rate_hz`, rounded to the
/// nearest frame (half a frame rounds up).
///
/// At 48 kHz a frame is 20.83 us, so any frame count is reached by giving its
/// duration to within 10 us; the frames actually applied are on the
/// `output-map` line the client prints.
pub fn delay_frames(delay_us: u64, rate_hz: u32) -> usize {
    ((delay_us * u64::from(rate_hz) + 500_000) / 1_000_000) as usize
}

/// Why an output map was refused. Each names the fix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MapError {
    /// `--output` was given without `--output-channels`.
    OutputsWithoutChannels,
    /// `--output-channels` was given without any `--output`.
    ChannelsWithoutOutputs {
        /// The count given.
        channels: u16,
    },
    /// The device channel count is outside 1 to 8.
    ChannelCount {
        /// The value as given.
        value: String,
    },
    /// An `--output` that is not `<index>=<source>[,gain-db=<dB>][,delay-us=<us>]`.
    Syntax {
        /// The spec as given.
        spec: String,
        /// What is wrong with it.
        why: String,
    },
    /// An output index outside the device's channels.
    IndexOutOfRange {
        /// The index given.
        index: u16,
        /// The device's channel count.
        channels: u16,
    },
    /// Two `--output`s for one device channel.
    DuplicateIndex {
        /// The index given twice.
        index: u16,
    },
    /// A name that is not a channel position.
    UnknownPosition {
        /// The name as given.
        name: String,
    },
    /// A downmix naming one position twice, or `MONO` with anything else.
    BadDownmix {
        /// The source as given.
        source: String,
        /// What is wrong with it.
        why: String,
    },
    /// A gain outside [`MIN_GAIN_DB`] to [`MAX_GAIN_DB`], or not a number.
    Gain {
        /// The value as given.
        value: String,
    },
    /// A delay above [`MAX_DELAY_US`], or not a whole number.
    Delay {
        /// The value as given.
        value: String,
    },
    /// Every output is silence.
    AllSilent,
    /// The device's frame is not the map's channel count in the stream's format.
    DeviceFrame {
        /// The device's frame length in bytes.
        device_frame_len: usize,
        /// The map's channel count.
        channels: u16,
        /// The stream's sample format.
        format: &'static str,
    },
}

impl fmt::Display for MapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let positions = ChannelPosition::ALL
            .iter()
            .map(|p| p.name())
            .collect::<Vec<_>>()
            .join(" ");
        match self {
            MapError::OutputsWithoutChannels => write!(
                f,
                "--output needs --output-channels <N>, the device's channel count (1 to {})",
                MAX_OUTPUT_CHANNELS
            ),
            MapError::ChannelsWithoutOutputs { channels } => write!(
                f,
                "--output-channels {} needs at least one --output <index>=<source>; an output \
                 not listed plays silence",
                channels
            ),
            MapError::ChannelCount { value } => write!(
                f,
                "--output-channels got '{}'; give the device's channel count, 1 to {}",
                value, MAX_OUTPUT_CHANNELS
            ),
            MapError::Syntax { spec, why } => write!(
                f,
                "--output '{}' {}; write it as <index>=<source>[,gain-db=<dB>][,delay-us=<us>], \
                 for example 2=FL+FR,gain-db=-3.0,delay-us=1200",
                spec, why
            ),
            MapError::IndexOutOfRange { index, channels } => write!(
                f,
                "--output {} is outside the device's {} channels; indices run 0 to {}",
                index,
                channels,
                channels.saturating_sub(1)
            ),
            MapError::DuplicateIndex { index } => write!(
                f,
                "device channel {} is given two --output lines; give each channel once",
                index
            ),
            MapError::UnknownPosition { name } => write!(
                f,
                "'{}' is not a channel position; use one of {} (docs/protocol.md, the channel \
                 map), a downmix such as FL+FR, or silence",
                name, positions
            ),
            MapError::BadDownmix { source, why } => write!(
                f,
                "the downmix '{}' {}; name each position once, and MONO only on its own",
                source, why
            ),
            MapError::Gain { value } => write!(
                f,
                "gain-db={} is not a gain between {} and +{} dB; for an output that is off use \
                 silence as its source",
                value, MIN_GAIN_DB, MAX_GAIN_DB
            ),
            MapError::Delay { value } => write!(
                f,
                "delay-us={} is not a whole number of microseconds from 0 to {}",
                value, MAX_DELAY_US
            ),
            MapError::AllSilent => write!(
                f,
                "every output of the map is silence; give at least one --output a position"
            ),
            MapError::DeviceFrame {
                device_frame_len,
                channels,
                format,
            } => write!(
                f,
                "the device's frame is {} bytes, which is not {} channels of {}; open the device \
                 with --output-channels channels",
                device_frame_len, channels, format
            ),
        }
    }
}

impl std::error::Error for MapError {}

/// The configured map: one [`OutputSpec`] per device channel.
#[derive(Debug, Clone, PartialEq)]
pub struct OutputMap {
    outputs: Vec<OutputSpec>,
}

impl OutputMap {
    /// A map over `channels` device channels. Channels not listed in
    /// `outputs` play silence.
    pub fn new(channels: u16, outputs: Vec<(u16, OutputSpec)>) -> Result<OutputMap, MapError> {
        if channels == 0 || channels > MAX_OUTPUT_CHANNELS {
            return Err(MapError::ChannelCount {
                value: channels.to_string(),
            });
        }
        if outputs.is_empty() {
            return Err(MapError::ChannelsWithoutOutputs { channels });
        }
        let mut slots: Vec<Option<OutputSpec>> = vec![None; usize::from(channels)];
        for (index, spec) in outputs {
            let slot = slots
                .get_mut(usize::from(index))
                .ok_or(MapError::IndexOutOfRange { index, channels })?;
            if slot.is_some() {
                return Err(MapError::DuplicateIndex { index });
            }
            check_source(&spec.source)?;
            if !(MIN_GAIN_DB..=MAX_GAIN_DB).contains(&spec.gain_db) {
                return Err(MapError::Gain {
                    value: spec.gain_db.to_string(),
                });
            }
            if spec.delay_us > MAX_DELAY_US {
                return Err(MapError::Delay {
                    value: spec.delay_us.to_string(),
                });
            }
            *slot = Some(spec);
        }
        let outputs: Vec<OutputSpec> = slots
            .into_iter()
            .map(|s| s.unwrap_or_else(OutputSpec::silence))
            .collect();
        if outputs.iter().all(|o| o.source == OutputSource::Silence) {
            return Err(MapError::AllSilent);
        }
        Ok(OutputMap { outputs })
    }

    /// The map from the command line's `--output-channels` and `--output`s,
    /// or `None` when neither was given (the client then plays the stream's
    /// channels as they are, exactly as it did before maps existed).
    pub fn from_args(
        channels: Option<&str>,
        outputs: &[String],
    ) -> Result<Option<OutputMap>, MapError> {
        let channels = match (channels, outputs.is_empty()) {
            (None, true) => return Ok(None),
            (None, false) => return Err(MapError::OutputsWithoutChannels),
            (Some(value), _) => value
                .parse::<u16>()
                .ok()
                .filter(|n| (1..=MAX_OUTPUT_CHANNELS).contains(n))
                .ok_or_else(|| MapError::ChannelCount {
                    value: value.to_string(),
                })?,
        };
        let parsed = outputs
            .iter()
            .map(|s| parse_output(s))
            .collect::<Result<Vec<_>, _>>()?;
        OutputMap::new(channels, parsed).map(Some)
    }

    /// Device channels this map drives.
    pub fn channels(&self) -> u16 {
        self.outputs.len() as u16
    }

    /// Each device channel's spec, in device order.
    pub fn outputs(&self) -> &[OutputSpec] {
        &self.outputs
    }

    /// The most stream channels worth sending this endpoint: the distinct
    /// positions its map reads, which is what `capabilities.max_channels`
    /// advertises when a map is configured. A stream with more would have
    /// channels this endpoint discards.
    pub fn max_stream_channels(&self) -> u8 {
        let mut seen: Vec<ChannelPosition> = Vec::new();
        for o in &self.outputs {
            for p in o.source.positions() {
                if !seen.contains(p) {
                    seen.push(*p);
                }
            }
        }
        seen.len().clamp(1, usize::from(MAX_OUTPUT_CHANNELS)) as u8
    }

    /// Resolve against one stream: which stream channel feeds each output,
    /// and what the stream lacks.
    pub fn resolve(
        &self,
        stream_map: &[ChannelPosition],
        format: SampleFormat,
        rate_hz: u32,
    ) -> ResolvedMap {
        let mono = stream_map == [ChannelPosition::Mono];
        let mut plans = Vec::with_capacity(self.outputs.len());
        let mut report = Vec::new();
        let mut missing = Vec::new();
        let mut used = vec![false; stream_map.len()];
        for (index, spec) in self.outputs.iter().enumerate() {
            let wanted = spec.source.positions();
            let mut inputs = Vec::new();
            let mut lacks = Vec::new();
            let mut fell_back = false;
            for p in wanted {
                match stream_map.iter().position(|s| s == p) {
                    Some(i) => inputs.push(i),
                    None if mono
                        && matches!(
                            p,
                            ChannelPosition::FrontLeft
                                | ChannelPosition::FrontRight
                                | ChannelPosition::FrontCenter
                        ) =>
                    {
                        inputs.push(0);
                        fell_back = true;
                    }
                    None => lacks.push(*p),
                }
            }
            for i in &inputs {
                used[*i] = true;
            }
            let frames = delay_frames(spec.delay_us, rate_hz);
            let passthrough = inputs.len() == 1 && wanted.len() == 1 && spec.gain_db == 0.0;
            report.push(format!(
                "output-map out={} source={} gain_db={} delay_us={} delay_frames={} inputs={}{}",
                index,
                spec.source.literal(),
                spec.gain_db,
                spec.delay_us,
                frames,
                if inputs.is_empty() {
                    "none".to_string()
                } else {
                    inputs
                        .iter()
                        .map(|i| i.to_string())
                        .collect::<Vec<_>>()
                        .join("+")
                },
                if passthrough { " exact=1" } else { "" }
            ));
            if fell_back {
                report.push(format!(
                    "output-map-fallback out={} source={} stream=MONO detail=a mono stream feeds \
                     every output that reads FL, FR or FC",
                    index,
                    spec.source.literal()
                ));
            }
            if !lacks.is_empty() {
                let names = lacks.iter().map(|p| p.name()).collect::<Vec<_>>();
                report.push(format!(
                    "output-map-missing out={} source={} stream_lacks={} stream={} silent={}",
                    index,
                    spec.source.literal(),
                    names.join("+"),
                    stream_literal(stream_map),
                    u8::from(inputs.is_empty())
                ));
                for p in lacks {
                    missing.push((index as u16, p));
                }
            }
            plans.push(ChannelPlan {
                inputs,
                weight: if wanted.is_empty() {
                    0.0
                } else {
                    1.0 / wanted.len() as f64
                },
                factor: 10f64.powf(spec.gain_db / 20.0),
                passthrough,
                delay_frames: frames,
            });
        }
        let unused: Vec<ChannelPosition> = stream_map
            .iter()
            .zip(&used)
            .filter(|(_, u)| !**u)
            .map(|(p, _)| *p)
            .collect();
        if !unused.is_empty() {
            report.push(format!(
                "output-map-unused stream={} unused={} detail=no output reads these stream channels",
                stream_literal(stream_map),
                unused.iter().map(|p| p.name()).collect::<Vec<_>>().join("+")
            ));
        }
        ResolvedMap {
            format,
            in_channels: stream_map.len(),
            plans,
            report,
            missing,
            unused,
        }
    }
}

fn stream_literal(stream_map: &[ChannelPosition]) -> String {
    stream_map
        .iter()
        .map(|p| p.name())
        .collect::<Vec<_>>()
        .join(",")
}

fn check_source(source: &OutputSource) -> Result<(), MapError> {
    if let OutputSource::Downmix(ps) = source {
        if ps.len() < 2 {
            return Err(MapError::BadDownmix {
                source: source.literal(),
                why: "names fewer than two positions".to_string(),
            });
        }
        for (i, p) in ps.iter().enumerate() {
            if ps[..i].contains(p) {
                return Err(MapError::BadDownmix {
                    source: source.literal(),
                    why: format!("names {} twice", p.name()),
                });
            }
        }
        if ps.contains(&ChannelPosition::Mono) {
            return Err(MapError::BadDownmix {
                source: source.literal(),
                why: "mixes MONO with other positions".to_string(),
            });
        }
    }
    Ok(())
}

/// Parse one `--output` value: `<index>=<source>[,gain-db=<dB>][,delay-us=<us>]`.
pub fn parse_output(spec: &str) -> Result<(u16, OutputSpec), MapError> {
    let syntax = |why: &str| MapError::Syntax {
        spec: spec.to_string(),
        why: why.to_string(),
    };
    let mut parts = spec.split(',');
    let head = parts.next().unwrap_or("");
    let (index, source) = head
        .split_once('=')
        .ok_or_else(|| syntax("has no <index>=<source>"))?;
    let index: u16 = index
        .trim()
        .parse()
        .map_err(|_| syntax("does not start with a device channel index"))?;
    let source = parse_source(source.trim())?;
    let mut out = OutputSpec {
        source,
        gain_db: 0.0,
        delay_us: 0,
    };
    let mut seen_gain = false;
    let mut seen_delay = false;
    for part in parts {
        let (key, value) = part
            .split_once('=')
            .ok_or_else(|| syntax("has a setting with no value"))?;
        match key.trim() {
            "gain-db" if !seen_gain => {
                seen_gain = true;
                let v = value.trim();
                out.gain_db = v
                    .parse::<f64>()
                    .ok()
                    .filter(|g| g.is_finite() && (MIN_GAIN_DB..=MAX_GAIN_DB).contains(g))
                    .ok_or_else(|| MapError::Gain {
                        value: v.to_string(),
                    })?;
            }
            "delay-us" if !seen_delay => {
                seen_delay = true;
                let v = value.trim();
                out.delay_us = v
                    .parse::<u64>()
                    .ok()
                    .filter(|d| *d <= MAX_DELAY_US)
                    .ok_or_else(|| MapError::Delay {
                        value: v.to_string(),
                    })?;
            }
            "gain-db" | "delay-us" => return Err(syntax("gives one setting twice")),
            _ => {
                return Err(syntax(&format!(
                    "has an unknown setting '{}' (the settings are gain-db and delay-us)",
                    key.trim()
                )))
            }
        }
    }
    Ok((index, out))
}

fn parse_source(text: &str) -> Result<OutputSource, MapError> {
    if text.eq_ignore_ascii_case("silence") {
        return Ok(OutputSource::Silence);
    }
    let names: Vec<&str> = text.split('+').map(str::trim).collect();
    let mut positions = Vec::with_capacity(names.len());
    for name in names {
        let p = ChannelPosition::from_name(&name.to_ascii_uppercase()).ok_or_else(|| {
            MapError::UnknownPosition {
                name: name.to_string(),
            }
        })?;
        positions.push(p);
    }
    let source = if positions.len() == 1 {
        OutputSource::Position(positions[0])
    } else {
        OutputSource::Downmix(positions)
    };
    check_source(&source)?;
    Ok(source)
}

/// How one device channel is computed.
#[derive(Debug, Clone)]
struct ChannelPlan {
    /// Stream channel indices summed (one per configured position found).
    inputs: Vec<usize>,
    /// Each input's weight: `1/n` of the positions configured.
    weight: f64,
    /// The gain as a linear factor.
    factor: f64,
    /// One position at 0 dB: a byte copy.
    passthrough: bool,
    /// Whole frames of delay.
    delay_frames: usize,
}

/// A map resolved against one stream.
#[derive(Debug, Clone)]
pub struct ResolvedMap {
    format: SampleFormat,
    in_channels: usize,
    plans: Vec<ChannelPlan>,
    report: Vec<String>,
    missing: Vec<(u16, ChannelPosition)>,
    unused: Vec<ChannelPosition>,
}

impl ResolvedMap {
    /// One line per output, and one per fallback, missing position and
    /// unused stream channel set: what the client prints at session start.
    pub fn report(&self) -> &[String] {
        &self.report
    }

    /// Each output that reads a position the stream lacks, with the position.
    pub fn missing(&self) -> &[(u16, ChannelPosition)] {
        &self.missing
    }

    /// Stream positions no output reads.
    pub fn unused(&self) -> &[ChannelPosition] {
        &self.unused
    }

    /// Each output's delay in whole frames, in device order.
    pub fn delays_frames(&self) -> Vec<usize> {
        self.plans.iter().map(|p| p.delay_frames).collect()
    }
}

/// The remap itself: stream frames in, device frames out, one for one.
#[derive(Debug)]
pub struct Remapper {
    map: ResolvedMap,
    bps: usize,
    /// One ring per output, `delay_frames * bps` bytes, and its position.
    lines: Vec<(Vec<u8>, usize)>,
    clipped: u64,
}

impl Remapper {
    /// A remapper for a resolved map.
    pub fn new(map: ResolvedMap) -> Remapper {
        let bps = map.format.bytes_per_sample();
        let lines = map
            .plans
            .iter()
            .map(|p| (vec![0u8; p.delay_frames * bps], 0usize))
            .collect();
        Remapper {
            map,
            bps,
            lines,
            clipped: 0,
        }
    }

    /// Bytes one stream frame occupies.
    pub fn in_frame_len(&self) -> usize {
        self.map.in_channels * self.bps
    }

    /// Bytes one device frame occupies.
    pub fn out_frame_len(&self) -> usize {
        self.map.plans.len() * self.bps
    }

    /// Samples saturated at full scale so far.
    pub fn clipped_samples(&self) -> u64 {
        self.clipped
    }

    /// Remap whole stream frames in `input` into `out` (cleared first). A
    /// trailing partial frame is ignored, as a device would ignore it.
    pub fn apply(&mut self, input: &[u8], out: &mut Vec<u8>) {
        let in_len = self.in_frame_len();
        let bps = self.bps;
        let frames = input.len().checked_div(in_len).unwrap_or(0);
        out.clear();
        out.resize(frames * self.out_frame_len(), 0);
        let mut sample = [0u8; 4];
        for f in 0..frames {
            let frame = &input[f * in_len..(f + 1) * in_len];
            for (c, plan) in self.map.plans.iter().enumerate() {
                let s = &mut sample[..bps];
                if plan.passthrough {
                    let i = plan.inputs[0] * bps;
                    s.copy_from_slice(&frame[i..i + bps]);
                } else if plan.inputs.is_empty() {
                    s.fill(0);
                } else {
                    let mut sum = 0.0f64;
                    for i in &plan.inputs {
                        sum += read(self.map.format, &frame[i * bps..(i + 1) * bps]);
                    }
                    let value = sum * plan.weight * plan.factor;
                    if write(self.map.format, value, s) {
                        self.clipped += 1;
                    }
                }
                let (ring, pos) = &mut self.lines[c];
                if !ring.is_empty() {
                    // The delay line: hand out the sample `k` frames old, keep
                    // this one in its place.
                    let held = &mut ring[*pos..*pos + bps];
                    for (a, b) in held.iter_mut().zip(s.iter_mut()) {
                        std::mem::swap(a, b);
                    }
                    *pos = (*pos + bps) % ring.len();
                }
                let o = (f * self.map.plans.len() + c) * bps;
                out[o..o + bps].copy_from_slice(s);
            }
        }
    }
}

/// One sample as a number: integer formats in their own units, float as is.
fn read(format: SampleFormat, b: &[u8]) -> f64 {
    match format {
        SampleFormat::PcmS16Le => f64::from(i16::from_le_bytes([b[0], b[1]])),
        SampleFormat::PcmS24Le => {
            let raw = i32::from_le_bytes([0, b[0], b[1], b[2]]) >> 8;
            f64::from(raw)
        }
        SampleFormat::PcmF32Le => f64::from(f32::from_le_bytes([b[0], b[1], b[2], b[3]])),
    }
}

/// Write `value` as one sample, rounded to nearest and saturated; true when it
/// saturated.
fn write(format: SampleFormat, value: f64, out: &mut [u8]) -> bool {
    match format {
        SampleFormat::PcmS16Le => {
            let r = value.round();
            let clipped = r > f64::from(i16::MAX) || r < f64::from(i16::MIN);
            let v = r.clamp(f64::from(i16::MIN), f64::from(i16::MAX)) as i16;
            out.copy_from_slice(&v.to_le_bytes());
            clipped
        }
        SampleFormat::PcmS24Le => {
            const MAX: f64 = 8_388_607.0;
            const MIN: f64 = -8_388_608.0;
            let r = value.round();
            let clipped = !(MIN..=MAX).contains(&r);
            let v = r.clamp(MIN, MAX) as i32;
            out.copy_from_slice(&v.to_le_bytes()[..3]);
            clipped
        }
        SampleFormat::PcmF32Le => {
            let clipped = !(-1.0..=1.0).contains(&value);
            let v = value.clamp(-1.0, 1.0) as f32;
            out.copy_from_slice(&v.to_le_bytes());
            clipped
        }
    }
}

/// A sink that plays the stream through an output map into a device with the
/// map's channel count.
///
/// It is the last edge before the device: it takes stream frames (the zone
/// gain already applied) and hands the device the same number of device
/// frames. Rate, delay, underruns, drains and the played-out count are the
/// device's own, passed through untouched, because a map changes what each
/// channel carries and never when a frame plays.
pub struct MappedSink<S: PcmSink> {
    inner: S,
    remap: Remapper,
    scratch: Vec<u8>,
    clipped: Arc<AtomicU64>,
}

impl<S: PcmSink> MappedSink<S> {
    /// Wrap `inner`, a device opened with `map.channels()` channels in the
    /// stream's format, for a stream whose channels are `stream_map`.
    pub fn new(
        inner: S,
        map: &OutputMap,
        stream_map: &[ChannelPosition],
        format: SampleFormat,
    ) -> Result<(MappedSink<S>, ResolvedMap), MapError> {
        let expected = usize::from(map.channels()) * format.bytes_per_sample();
        if inner.frame_len() != expected {
            return Err(MapError::DeviceFrame {
                device_frame_len: inner.frame_len(),
                channels: map.channels(),
                format: format.name(),
            });
        }
        let resolved = map.resolve(stream_map, format, inner.rate_hz());
        Ok((
            MappedSink {
                inner,
                remap: Remapper::new(resolved.clone()),
                scratch: Vec::new(),
                clipped: Arc::new(AtomicU64::new(0)),
            },
            resolved,
        ))
    }

    /// Samples saturated at full scale so far.
    pub fn clipped_samples(&self) -> u64 {
        self.remap.clipped_samples()
    }

    /// The same count, readable after the sink has been handed away.
    pub fn clip_counter(&self) -> Arc<AtomicU64> {
        Arc::clone(&self.clipped)
    }

    /// The device underneath.
    pub fn inner(&self) -> &S {
        &self.inner
    }

    /// The device underneath, mutably.
    pub fn inner_mut(&mut self) -> &mut S {
        &mut self.inner
    }
}

impl<S: PcmSink> PcmSink for MappedSink<S> {
    fn device(&self) -> &str {
        self.inner.device()
    }

    /// The STREAM's frame: what the client hands this sink.
    fn frame_len(&self) -> usize {
        self.remap.in_frame_len()
    }

    fn rate_hz(&self) -> u32 {
        self.inner.rate_hz()
    }

    fn write(&mut self, pcm: &[u8]) -> Result<SinkWrite, SinkError> {
        self.remap.apply(pcm, &mut self.scratch);
        self.clipped
            .store(self.remap.clipped_samples(), Ordering::Relaxed);
        self.inner.write(&self.scratch)
    }

    fn delay_frames(&mut self) -> Result<i64, SinkError> {
        self.inner.delay_frames()
    }

    fn fixed_latency_frames(&self) -> i64 {
        self.inner.fixed_latency_frames()
    }

    fn in_xrun(&mut self) -> Result<bool, SinkError> {
        self.inner.in_xrun()
    }

    fn drain(&mut self) -> Result<(), SinkError> {
        self.inner.drain()
    }

    fn frames_played(&mut self) -> Result<u64, SinkError> {
        self.inner.frames_played()
    }
}

/// The device channel count the client opens ALSA with: the map's, or the
/// stream's when there is no map (today's behaviour).
pub fn device_channels(map: Option<&OutputMap>, stream_channels: u16) -> u16 {
    map.map(OutputMap::channels).unwrap_or(stream_channels)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delay_rounds_to_the_nearest_whole_frame() {
        assert_eq!(delay_frames(1_000, 48_000), 48);
        assert_eq!(delay_frames(0, 48_000), 0);
        assert_eq!(delay_frames(10, 48_000), 0);
        assert_eq!(delay_frames(11, 48_000), 1);
        assert_eq!(delay_frames(MAX_DELAY_US, 384_000), 19_200);
    }

    #[test]
    fn a_spec_parses_with_and_without_settings() {
        let (i, s) = parse_output("2=FL+FR,gain-db=-3.0,delay-us=1200").unwrap();
        assert_eq!(i, 2);
        assert_eq!(
            s.source,
            OutputSource::Downmix(vec![
                ChannelPosition::FrontLeft,
                ChannelPosition::FrontRight
            ])
        );
        assert_eq!(s.gain_db, -3.0);
        assert_eq!(s.delay_us, 1_200);
        let (i, s) = parse_output("0=lfe").unwrap();
        assert_eq!(
            (i, s.source),
            (0, OutputSource::Position(ChannelPosition::LowFrequency))
        );
    }

    #[test]
    fn the_advertised_channel_count_is_the_distinct_positions_read() {
        let map =
            OutputMap::from_args(Some("4"), &["0=FL".into(), "1=FR".into(), "2=FL+FR".into()])
                .unwrap()
                .unwrap();
        assert_eq!(map.max_stream_channels(), 2);
    }
}
