//! The endpoint's sound chain (goal 12) at the sink's edge: `crates/dsp`'s
//! [`Chain`], configured from the room's `sound` (protocol v2 0x39), the
//! room's volume and limit, and this endpoint's own two-way drivers
//! (`--two-way`).
//!
//! # Where it sits, and why
//!
//! [`DspSink`] wraps the device, where [`crate::outmap::MappedSink`] does
//! (ADR 0068), and takes over the output map's job. The playout loop hands it
//! STREAM frames; it hands the device the same number of DEVICE frames. In
//! between:
//!
//! 1. the stream's samples to `f32`;
//! 2. the chain, which needs the WHOLE stream (a subwoofer's feed is the low
//!    branch of the sum of the stream's main channels, so the chain cannot
//!    run after a map has picked one position), with the room gain inside it
//!    (the library's order, `docs/dsp.md`: the volume after the sound stages
//!    and before the look-ahead limiter, whose ceiling is
//!    `min(1, the room's effective limit)`, K81, I10);
//! 3. the chain's outputs onto the device: a two-way's woofer and tweeter onto
//!    the outputs `--two-way` names; otherwise, with an output map, the map
//!    resolved against the chain's OUTPUT positions (the stream's own when the
//!    endpoint is in no set, `[role]` when it plays one role of a bonded set,
//!    so `--output 0=LFE` picks up a subwoofer's feed on a stereo stream that
//!    has no LFE channel at all); with neither, the outputs as they are, or
//!    one output on every device channel.
//!
//! So a subwoofer on an output map and a two-way on flags both work, and the
//! map is re-resolved whenever a `sound` moves the endpoint's role. Before
//! the chain engages this is exactly `MappedSink` (or the device itself):
//! byte for byte today's client.
//!
//! # When it engages
//!
//! When the endpoint has a two-way, or has received a `sound`. The server
//! sends `sound` in its greeting, before any audio (ADR 0081), so in practice
//! the chain is in the path from a stream's first frame. Nothing configures
//! it before that, so an endpoint of a server that never sends `sound` plays
//! as it did before goal 12. A first `sound` that arrives mid-stream engages
//! it there: once, a 2 ms step the sync loop takes up like any other.
//!
//! # Time
//!
//! The chain always holds the limiter's look-ahead (2 ms, `docs/dsp.md`),
//! whatever the settings, so a setting never moves the audio in time. That
//! latency is part of how far a frame written now is from the DAC, so
//! [`PcmSink::delay_frames`] reports the device's delay PLUS the chain's: the
//! sync loop then writes each frame that much earlier and it is heard at the
//! sync target. The C endpoint adds the same frames to its device delay
//! (`firmware/src/playout.c`), so both endpoint kinds account it one way.
//!
//! # The gain, frame by frame
//!
//! The room's gain ramps per frame (`crate::zone::RoomGain`). While the chain
//! is engaged, [`crate::control::ZoneWatch::apply`] leaves the samples alone
//! and queues each frame's Q16 gain; the chain takes them in blocks of
//! [`GAIN_BLOCK_FRAMES`], each at its first frame's gain (a ramp becomes a
//! staircase of 32-frame steps, 0.67 ms at 48 kHz). The C endpoint does the
//! same with the same block.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use chorus_dsp::settings::TWO_WAY_EXAMPLE_HZ;
use chorus_dsp::{Chain, Driver, DspError, EndpointDsp, RoomEqFilter, SoundSettings, TwoWay};
use chorus_protocol::v2::{sound_flags, ChannelPosition, Sound};
use chorus_protocol::SampleFormat;

use crate::control::ZoneWatch;
use crate::outmap::{OutputMap, Remapper};
use crate::sink::{PcmSink, SinkError, SinkWrite};
use crate::zone::UNITY_Q16;

/// Frames the chain is run over at one room gain while the gain ramps.
/// ASSUMED: 32, short enough that a ramp's steps are inaudible (the C
/// endpoint's `CHORUS_ENDPOINT_DSP_BLOCK_FRAMES` is the same).
pub const GAIN_BLOCK_FRAMES: usize = 32;

/// The most device outputs a two-way may name: the protocol's 8 channels.
pub const MAX_TWO_WAY_OUTPUT: u16 = 7;

/// The endpoint's two-way drivers on the command line:
/// `--two-way crossover-hz=<Hz>,woofer=<output>,tweeter=<output>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TwoWayOutputs {
    /// The LR4 crossover between the drivers, Hz.
    pub crossover_hz: u32,
    /// The device output the woofer is wired to.
    pub woofer: u16,
    /// The device output the tweeter is wired to.
    pub tweeter: u16,
}

impl Default for TwoWayOutputs {
    /// The design envelope's example until goals 24-25 design the drivers:
    /// 2 kHz, the woofer on output 0, the tweeter on output 1. ASSUMED.
    fn default() -> TwoWayOutputs {
        TwoWayOutputs {
            crossover_hz: TWO_WAY_EXAMPLE_HZ,
            woofer: 0,
            tweeter: 1,
        }
    }
}

impl TwoWayOutputs {
    /// Parse `crossover-hz=<Hz>,woofer=<output>,tweeter=<output>`; any key may
    /// be left out and takes [`TwoWayOutputs::default`]'s value. Refusals say
    /// which part was wrong and what would be right.
    pub fn parse(text: &str) -> Result<TwoWayOutputs, String> {
        let mut t = TwoWayOutputs::default();
        for part in text.split(',').map(str::trim).filter(|p| !p.is_empty()) {
            let (key, value) = part
                .split_once('=')
                .ok_or_else(|| format!("'{}' is not key=value", part))?;
            let number = |what: &str| -> Result<u32, String> {
                value
                    .trim()
                    .parse::<u32>()
                    .map_err(|_| format!("{} got '{}', which is not a whole number", what, value))
            };
            match key.trim() {
                "crossover-hz" => t.crossover_hz = number("crossover-hz")?,
                "woofer" => t.woofer = output(number("woofer")?, "woofer")?,
                "tweeter" => t.tweeter = output(number("tweeter")?, "tweeter")?,
                other => {
                    return Err(format!(
                        "unknown key '{}'; the keys are crossover-hz, woofer and tweeter",
                        other
                    ))
                }
            }
        }
        // The chain's own corner rule is checked against the stream's rate
        // when the stream is known; 20 Hz is the floor whatever the rate.
        if t.crossover_hz < 20 || t.crossover_hz > 20_000 {
            return Err(format!(
                "crossover-hz {} is outside 20..=20000 (and must also be at most 0.45 x the \
                 stream's rate)",
                t.crossover_hz
            ));
        }
        if t.woofer == t.tweeter {
            return Err(format!(
                "woofer and tweeter are both output {}; each driver needs its own",
                t.woofer
            ));
        }
        Ok(t)
    }

    /// The device channels a two-way needs: enough to reach both drivers.
    pub fn device_channels(&self) -> u16 {
        self.woofer.max(self.tweeter) + 1
    }

    /// The chain's endpoint configuration: the split, drivers untrimmed,
    /// undelayed and not inverted (goals 24-25 give those).
    pub fn endpoint(&self) -> EndpointDsp {
        EndpointDsp {
            two_way: Some(TwoWay {
                crossover_hz: self.crossover_hz,
                woofer: Driver::default(),
                tweeter: Driver::default(),
            }),
            ..EndpointDsp::default()
        }
    }
}

fn output(n: u32, what: &str) -> Result<u16, String> {
    if n > u32::from(MAX_TWO_WAY_OUTPUT) {
        return Err(format!(
            "{} is output {}; outputs are 0 to {}",
            what, n, MAX_TWO_WAY_OUTPUT
        ));
    }
    Ok(n as u16)
}

/// The device channel count the client opens ALSA with: the map's, a two-way's
/// (at least the stream's), or the stream's (today's behaviour).
pub fn device_channels(
    map: Option<&OutputMap>,
    two_way: Option<&TwoWayOutputs>,
    stream_channels: u16,
) -> u16 {
    match (map, two_way) {
        (Some(m), _) => m.channels(),
        (None, Some(t)) => t.device_channels().max(stream_channels),
        (None, None) => stream_channels,
    }
}

/// The wire's `sound` as the chain's settings: the same fields, the flags
/// byte unpacked.
pub fn settings_from(sound: &Sound) -> SoundSettings {
    let flag = |bit: u8| sound.flags & bit != 0;
    SoundSettings {
        bass_db: sound.bass_db,
        treble_db: sound.treble_db,
        loudness: flag(sound_flags::LOUDNESS),
        night: flag(sound_flags::NIGHT),
        speech: flag(sound_flags::SPEECH),
        room_eq_enabled: flag(sound_flags::ROOM_EQ),
        sub_polarity_inverted: flag(sound_flags::SUB_INVERTED),
        role: sound.role,
        sub_present: sound.sub_present,
        crossover_hz: sound.crossover_hz,
        sub_level_cdb: sound.sub_level_cdb,
        room_eq: sound
            .filters
            .iter()
            .map(|f| RoomEqFilter {
                freq_hz: f.freq_hz,
                gain_cdb: f.gain_cdb,
                q_milli: f.q_milli,
            })
            .collect(),
    }
}

/// The positions the chain's outputs carry, for an output map to resolve
/// against: the stream's own when not in a set, else the one role. `None`
/// for a two-way, whose outputs are drivers, not positions.
fn output_positions(
    settings: &SoundSettings,
    two_way: bool,
    stream: &[ChannelPosition],
) -> Option<Vec<ChannelPosition>> {
    if two_way {
        return None;
    }
    if settings.role == 0 {
        return Some(stream.to_vec());
    }
    ChannelPosition::from_wire(settings.role).map(|p| vec![p])
}

/// Counters a run prints at its end.
#[derive(Debug, Default)]
pub struct DspCounters {
    /// `sound`s applied to the chain.
    pub sounds_applied: AtomicU64,
    /// `sound`s the chain refused (the previous settings were kept).
    pub refusals: AtomicU64,
    /// Samples saturated at full scale on the way to the device.
    pub clipped_samples: AtomicU64,
    /// Whether the chain is in the path (0 or 1).
    pub engaged: AtomicU64,
}

impl DspCounters {
    /// One status line.
    pub fn line(&self) -> String {
        format!(
            "dsp engaged={} sounds_applied={} refusals={} clipped_samples={}",
            self.engaged.load(Ordering::Relaxed),
            self.sounds_applied.load(Ordering::Relaxed),
            self.refusals.load(Ordering::Relaxed),
            self.clipped_samples.load(Ordering::Relaxed)
        )
    }
}

/// The device behind the endpoint's chain (and its output map). See the
/// module.
pub struct DspSink<S: PcmSink> {
    inner: S,
    watch: Arc<ZoneWatch>,
    format: SampleFormat,
    stream_map: Vec<ChannelPosition>,
    stream_wire: Vec<u8>,
    device_channels: usize,
    map: Option<OutputMap>,
    two_way: Option<TwoWayOutputs>,
    chain: Option<Chain>,
    sounds_seen: u64,
    /// The map resolved against what the device is fed: the stream (not
    /// engaged) or the chain's outputs (engaged).
    remap: Option<Remapper>,
    remap_positions: Vec<ChannelPosition>,
    input: Vec<f32>,
    output: Vec<f32>,
    staged: Vec<u8>,
    device: Vec<u8>,
    counters: Arc<DspCounters>,
    report: Vec<String>,
}

impl<S: PcmSink> DspSink<S> {
    /// Wrap `inner`, a device opened with [`device_channels`] channels in the
    /// stream's format, for a stream of `stream_map` in `format`. Engages at
    /// once when there is a two-way or `watch` already holds a `sound`.
    pub fn new(
        inner: S,
        map: Option<&OutputMap>,
        two_way: Option<&TwoWayOutputs>,
        stream_map: &[ChannelPosition],
        format: SampleFormat,
        watch: Arc<ZoneWatch>,
    ) -> Result<DspSink<S>, String> {
        let bytes = format.bytes_per_sample();
        if !inner.frame_len().is_multiple_of(bytes) || inner.frame_len() == 0 {
            return Err(format!(
                "the device takes {}-byte frames, not whole {} samples",
                inner.frame_len(),
                format.name()
            ));
        }
        let device_channels = inner.frame_len() / bytes;
        if let Some(m) = map {
            if usize::from(m.channels()) != device_channels {
                return Err(format!(
                    "the output map has {} channels and the device {}",
                    m.channels(),
                    device_channels
                ));
            }
        }
        if let Some(t) = two_way {
            if map.is_some() {
                return Err(
                    "--two-way names the device outputs itself; it is not combined \
                            with --output"
                        .to_string(),
                );
            }
            if usize::from(t.device_channels()) > device_channels {
                return Err(format!(
                    "--two-way drives output {} and the device has {} channels",
                    t.woofer.max(t.tweeter),
                    device_channels
                ));
            }
        }
        let mut sink = DspSink {
            inner,
            watch,
            format,
            stream_map: stream_map.to_vec(),
            stream_wire: stream_map.iter().map(|p| p.to_wire()).collect(),
            device_channels,
            map: map.cloned(),
            two_way: two_way.copied(),
            chain: None,
            sounds_seen: 0,
            remap: None,
            remap_positions: Vec::new(),
            input: Vec::new(),
            output: Vec::new(),
            staged: Vec::new(),
            device: Vec::new(),
            counters: Arc::new(DspCounters::default()),
            report: Vec::new(),
        };
        // Not engaged yet: the map against the stream, exactly MappedSink.
        let positions = sink.stream_map.clone();
        sink.resolve(&positions);
        sink.refresh();
        if sink.chain.is_none() && sink.two_way.is_some() {
            return Err(sink.report.last().cloned().unwrap_or_else(|| {
                "the two-way could not be configured for this stream".to_string()
            }));
        }
        Ok(sink)
    }

    /// What was resolved and decided, for the client's status lines.
    pub fn report(&self) -> &[String] {
        &self.report
    }

    /// The counters, readable after the sink has been handed away.
    pub fn counters(&self) -> Arc<DspCounters> {
        Arc::clone(&self.counters)
    }

    /// The chain, when engaged (for the tests).
    pub fn chain(&self) -> Option<&Chain> {
        self.chain.as_ref()
    }

    /// The device underneath.
    pub fn inner(&self) -> &S {
        &self.inner
    }

    /// Resolve the output map (if any) against `positions`.
    fn resolve(&mut self, positions: &[ChannelPosition]) {
        if positions == self.remap_positions.as_slice() && self.remap.is_some() {
            return;
        }
        if let Some(m) = &self.map {
            let resolved = m.resolve(positions, self.format, self.inner.rate_hz());
            for line in resolved.report() {
                self.report.push(line.clone());
            }
            self.remap = Some(Remapper::new(resolved));
        }
        self.remap_positions = positions.to_vec();
    }

    /// Engage or reconfigure from the watch's last `sound` when a new one has
    /// arrived (or at the start). A refused one keeps what was in force.
    fn refresh(&mut self) {
        let seen = self.watch.sounds_received();
        let first = self.chain.is_none();
        if !first && seen == self.sounds_seen {
            return;
        }
        let sound = self.watch.last_sound();
        if first && sound.is_none() && self.two_way.is_none() {
            return;
        }
        self.sounds_seen = seen;
        let settings = sound.as_ref().map(settings_from).unwrap_or_default();
        let outcome = match &mut self.chain {
            Some(chain) => chain.set_sound(&settings),
            None => {
                let endpoint = self.two_way.map(|t| t.endpoint()).unwrap_or_default();
                match Chain::new(
                    &settings,
                    &endpoint,
                    &self.stream_wire,
                    self.inner.rate_hz(),
                ) {
                    Ok(chain) => {
                        self.report.push(format!(
                            "dsp engaged latency_frames={} outputs={} two_way={}",
                            chain.latency_frames(),
                            chain.out_channels(),
                            self.two_way
                                .map(|t| format!(
                                    "crossover_hz={},woofer={},tweeter={}",
                                    t.crossover_hz, t.woofer, t.tweeter
                                ))
                                .unwrap_or_else(|| "none".to_string())
                        ));
                        self.chain = Some(chain);
                        self.counters.engaged.store(1, Ordering::Relaxed);
                        // From here the gain is applied inside the chain.
                        self.watch.defer_gain_to_dsp(true);
                        Ok(())
                    }
                    Err(e) => Err(e),
                }
            }
        };
        match outcome {
            Ok(()) => {
                if sound.is_some() {
                    self.counters.sounds_applied.fetch_add(1, Ordering::Relaxed);
                }
            }
            Err(e) => {
                self.counters.refusals.fetch_add(1, Ordering::Relaxed);
                self.report.push(refused_line(&e));
            }
        }
        if let Some(chain) = &self.chain {
            if let Some(positions) =
                output_positions(chain.settings(), self.two_way.is_some(), &self.stream_map)
            {
                self.resolve(&positions);
            }
        }
    }

    /// Stream bytes to `f32` in `self.input`.
    fn decode(&mut self, pcm: &[u8]) {
        self.input.clear();
        match self.format {
            SampleFormat::PcmS16Le => self.input.extend(
                pcm.as_chunks::<2>()
                    .0
                    .iter()
                    .map(|s| f32::from(i16::from_le_bytes(*s)) * (1.0 / 32768.0)),
            ),
            SampleFormat::PcmS24Le => self.input.extend(pcm.as_chunks::<3>().0.iter().map(|s| {
                let v = (i32::from_le_bytes([0, s[0], s[1], s[2]])) >> 8;
                v as f32 * (1.0 / 8_388_608.0)
            })),
            SampleFormat::PcmF32Le => self.input.extend(
                pcm.as_chunks::<4>()
                    .0
                    .iter()
                    .map(|s| f32::from_le_bytes(*s)),
            ),
        }
    }

    /// One `f32` sample in the stream's format, rounded to the nearest and
    /// saturated, never wrapped (ADR 0068's rule).
    fn encode(&self, v: f32, out: &mut Vec<u8>) -> bool {
        let (clipped, bytes): (bool, [u8; 4]) = match self.format {
            SampleFormat::PcmS16Le => {
                let x = (f64::from(v) * 32768.0).round();
                let c = x.clamp(-32768.0, 32767.0);
                (c != x || v.is_nan(), (c as i32).to_le_bytes())
            }
            SampleFormat::PcmS24Le => {
                let x = (f64::from(v) * 8_388_608.0).round();
                let c = x.clamp(-8_388_608.0, 8_388_607.0);
                (c != x || v.is_nan(), (c as i32).to_le_bytes())
            }
            SampleFormat::PcmF32Le => {
                let c = if v.is_nan() { 0.0 } else { v.clamp(-1.0, 1.0) };
                (c != v, c.to_le_bytes())
            }
        };
        out.extend_from_slice(&bytes[..self.format.bytes_per_sample()]);
        clipped
    }

    /// Run the chain over the decoded input, gain block by gain block, into
    /// `self.output`.
    fn run_chain(&mut self, frames: usize) -> Result<(), DspError> {
        let gains = self.watch.take_dsp_gains();
        let room = self.watch.room();
        let settled = room.applied_q16(self.watch.gain());
        let limit_q16 = crate::zone::q16_from_thousandths(
            room.limit().thousandths().min(room.ceiling().thousandths()),
        );
        let limit = limit_q16 as f32 / UNITY_Q16 as f32;
        let chain = match &mut self.chain {
            Some(c) => c,
            None => return Ok(()),
        };
        let n = chain.in_channels();
        let k = chain.out_channels();
        self.output.resize(frames * k, 0.0);
        let mut at = 0;
        while at < frames {
            let q16 = gains.get(at).copied().unwrap_or(settled);
            // While the ramp is still, one call for the whole write.
            let mut len = GAIN_BLOCK_FRAMES.min(frames - at);
            if gains.len() <= at || gains[at..].iter().all(|&g| g == q16) {
                len = frames - at;
            }
            let gain = q16 as f32 / UNITY_Q16 as f32;
            chain.process(
                &self.input[at * n..(at + len) * n],
                &mut self.output[at * k..(at + len) * k],
                gain,
                limit,
            )?;
            at += len;
        }
        Ok(())
    }
}

fn refused_line(e: &DspError) -> String {
    format!("dsp refused detail={}; the settings in force are kept", e)
}

impl<S: PcmSink> PcmSink for DspSink<S> {
    fn device(&self) -> &str {
        self.inner.device()
    }

    /// The STREAM's frame: what the client hands this sink.
    fn frame_len(&self) -> usize {
        self.stream_map.len().max(1) * self.format.bytes_per_sample()
    }

    fn rate_hz(&self) -> u32 {
        self.inner.rate_hz()
    }

    fn write(&mut self, pcm: &[u8]) -> Result<SinkWrite, SinkError> {
        self.refresh();
        if self.chain.is_none() {
            // Not engaged: today's client, through the map if there is one.
            return match &mut self.remap {
                Some(remap) => {
                    let before = remap.clipped_samples();
                    remap.apply(pcm, &mut self.device);
                    self.counters
                        .clipped_samples
                        .fetch_add(remap.clipped_samples() - before, Ordering::Relaxed);
                    self.inner.write(&self.device)
                }
                None => self.inner.write(pcm),
            };
        }
        let frames = pcm.len() / self.frame_len();
        self.decode(&pcm[..frames * self.frame_len()]);
        if let Err(e) = self.run_chain(frames) {
            // Only a buffer of whole frames reaches here, so this is a bug
            // and not a setting: the device is told rather than fed garbage.
            return Err(SinkError::Modelled(format!(
                "the endpoint DSP failed: {}",
                e
            )));
        }
        let k = self.chain.as_ref().map(Chain::out_channels).unwrap_or(1);
        let d = self.device_channels;
        let mut clipped = 0u64;
        self.device.clear();
        self.staged.clear();
        let mut staged = std::mem::take(&mut self.staged);
        let mut device = std::mem::take(&mut self.device);
        if let Some(t) = self.two_way {
            // The drivers onto the outputs they are wired to; every other
            // output silence.
            for f in 0..frames {
                for o in 0..d {
                    let v = if o == usize::from(t.woofer) {
                        self.output[f * k]
                    } else if o == usize::from(t.tweeter) && k > 1 {
                        self.output[f * k + 1]
                    } else {
                        0.0
                    };
                    clipped += u64::from(self.encode(v, &mut device));
                }
            }
        } else if self.remap.is_some() {
            for &v in &self.output[..frames * k] {
                clipped += u64::from(self.encode(v, &mut staged));
            }
            if let Some(remap) = &mut self.remap {
                let before = remap.clipped_samples();
                remap.apply(&staged, &mut device);
                clipped += remap.clipped_samples() - before;
            }
        } else {
            // No map: the outputs as they are when they fill the device, one
            // output on every channel (a bonded member, a sub), else the
            // first outputs and silence.
            for f in 0..frames {
                for o in 0..d {
                    let v = if k == d {
                        self.output[f * k + o]
                    } else if k == 1 {
                        self.output[f]
                    } else if o < k {
                        self.output[f * k + o]
                    } else {
                        0.0
                    };
                    clipped += u64::from(self.encode(v, &mut device));
                }
            }
        }
        self.counters
            .clipped_samples
            .fetch_add(clipped, Ordering::Relaxed);
        let result = self.inner.write(&device);
        self.staged = staged;
        self.device = device;
        result
    }

    /// The device's delay plus the chain's look-ahead: both are between a
    /// frame written now and the DAC (see the module).
    fn delay_frames(&mut self) -> Result<i64, SinkError> {
        let latency = self
            .chain
            .as_ref()
            .map(|c| c.latency_frames() as i64)
            .unwrap_or(0);
        Ok(self.inner.delay_frames()? + latency)
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

impl<S: PcmSink> Drop for DspSink<S> {
    /// The gain goes back to the playout loop: the next stream's sink takes
    /// it again if it engages.
    fn drop(&mut self) {
        if self.chain.is_some() {
            self.watch.defer_gain_to_dsp(false);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_way_flags_parse_with_defaults_and_refuse_by_name() {
        assert_eq!(
            TwoWayOutputs::parse("").unwrap(),
            TwoWayOutputs {
                crossover_hz: 2000,
                woofer: 0,
                tweeter: 1
            }
        );
        assert_eq!(
            TwoWayOutputs::parse("crossover-hz=2500,woofer=1,tweeter=0").unwrap(),
            TwoWayOutputs {
                crossover_hz: 2500,
                woofer: 1,
                tweeter: 0
            }
        );
        assert!(TwoWayOutputs::parse("woofer=1,tweeter=1")
            .unwrap_err()
            .contains("own"));
        assert!(TwoWayOutputs::parse("crossover-hz=10")
            .unwrap_err()
            .contains("20..=20000"));
        assert!(TwoWayOutputs::parse("woofer=9")
            .unwrap_err()
            .contains("0 to 7"));
        assert!(TwoWayOutputs::parse("mid=2")
            .unwrap_err()
            .contains("unknown key"));
    }

    #[test]
    fn the_wire_flags_unpack_into_the_chain_settings() {
        let s = Sound {
            bass_db: 3,
            treble_db: -2,
            flags: sound_flags::NIGHT | sound_flags::SUB_INVERTED | sound_flags::ROOM_EQ,
            role: 4,
            sub_present: true,
            crossover_hz: 100,
            sub_level_cdb: -350,
            filters: vec![chorus_protocol::v2::SoundFilter {
                freq_hz: 42,
                gain_cdb: -600,
                q_milli: 4500,
            }],
        };
        let t = settings_from(&s);
        assert!(t.night && t.sub_polarity_inverted && t.room_eq_enabled);
        assert!(!t.loudness && !t.speech);
        assert_eq!((t.bass_db, t.treble_db, t.role), (3, -2, 4));
        assert_eq!((t.crossover_hz, t.sub_level_cdb), (100, -350));
        assert_eq!(t.room_eq.len(), 1);
        assert!(t.validate().is_ok());
    }

    #[test]
    fn a_role_is_the_one_position_an_output_map_resolves_against() {
        let stereo = [ChannelPosition::FrontLeft, ChannelPosition::FrontRight];
        let mut s = SoundSettings::default();
        assert_eq!(output_positions(&s, false, &stereo).unwrap(), stereo);
        s.role = 4;
        assert_eq!(
            output_positions(&s, false, &stereo).unwrap(),
            [ChannelPosition::from_wire(4).unwrap()]
        );
        assert_eq!(output_positions(&s, true, &stereo), None);
    }
}
