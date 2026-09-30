//! Goal 10 line A: `chorus-client` plays N channels with per-channel maps,
//! gains and delays, on fake devices.
//!
//! Every assertion here is on the bytes a modelled N-channel device ACCEPTED,
//! read back channel by channel. The device lives under `tests/`, where no
//! binary can reach it (`crates/client-linux/src/sink.rs`); the map under test
//! is the one the shipped client wraps its ALSA sink in
//! (`chorus_client_linux::outmap::MappedSink`).
//!
//! The 5.1 stream is FL FR FC LFE SL SR with a distinct signal on every
//! channel; the device is 8 channels in a different order: ALSA's
//! `surround51` order (FL FR RL RR FC LFE, `docs/protocol.md` "The channel
//! map", where that order is a LEAD; the stream's side pair plays on the
//! device's rear pair) plus two extra outputs.

// The checks read several parallel per-channel vectors at one frame index,
// which an index says more plainly than a zip of eight iterators.
#![allow(clippy::needless_range_loop)]

mod common;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use chorus_audio::MonotonicTimeline;
use chorus_client_linux::config::{ClientConfig, ConfigError};
use chorus_client_linux::control::ZoneWatch;
use chorus_client_linux::delaylog::DelayLog;
use chorus_client_linux::outmap::{
    delay_frames, parse_output, MapError, MappedSink, OutputMap, MAX_DELAY_US, MAX_GAIN_DB,
    MIN_GAIN_DB,
};
use chorus_client_linux::receive::{Handshake, StreamShape};
use chorus_client_linux::run::{fresh_receiver, header_for, run_session, StopReason};
use chorus_client_linux::session::capabilities;
use chorus_client_linux::sink::{PcmSink, SinkError, SinkWrite};
use chorus_client_linux::Counters;
use chorus_protocol::v2::ChannelPosition::{self, *};
use chorus_protocol::{encode, AudioChunk, Message, SampleFormat, StreamEnd, RESERVED_LEN};

use common::PacedReader;

const RATE_HZ: u32 = 48_000;

/// The 5.1 stream's order, as the task and a WAV-style source give it.
const FIVE_ONE: [ChannelPosition; 6] = [
    FrontLeft,
    FrontRight,
    FrontCenter,
    LowFrequency,
    SideLeft,
    SideRight,
];

/// A modelled N-channel device that keeps every byte it was handed.
///
/// Its ring drains at the nominal rate against the monotonic clock, as the
/// zone test's recording device does, because the playout loop paces its
/// writes against the delay a device reports.
struct RecordingDevice {
    channels: usize,
    format: SampleFormat,
    tape: Arc<Mutex<Vec<u8>>>,
    queued: f64,
    played: u64,
    last_tick: std::time::Instant,
}

impl RecordingDevice {
    fn new(channels: usize, format: SampleFormat) -> RecordingDevice {
        RecordingDevice {
            channels,
            format,
            tape: Arc::new(Mutex::new(Vec::new())),
            queued: 0.0,
            played: 0,
            last_tick: std::time::Instant::now(),
        }
    }

    fn tape(&self) -> Arc<Mutex<Vec<u8>>> {
        Arc::clone(&self.tape)
    }

    fn tick(&mut self) {
        let now = std::time::Instant::now();
        let elapsed = now.duration_since(self.last_tick).as_secs_f64();
        self.last_tick = now;
        let consumed = (elapsed * f64::from(RATE_HZ)).min(self.queued).max(0.0);
        self.queued -= consumed;
        self.played += consumed as u64;
    }
}

impl PcmSink for RecordingDevice {
    fn device(&self) -> &str {
        "modelled-n-channel"
    }
    fn frame_len(&self) -> usize {
        self.channels * self.format.bytes_per_sample()
    }
    fn rate_hz(&self) -> u32 {
        RATE_HZ
    }
    fn write(&mut self, pcm: &[u8]) -> Result<SinkWrite, SinkError> {
        self.tick();
        assert_eq!(
            pcm.len() % self.frame_len(),
            0,
            "the device is handed whole device frames"
        );
        self.tape.lock().unwrap().extend_from_slice(pcm);
        let frames = (pcm.len() / self.frame_len()) as u64;
        self.queued += frames as f64;
        Ok(SinkWrite {
            frames_written: frames,
            underran: false,
        })
    }
    fn delay_frames(&mut self) -> Result<i64, SinkError> {
        self.tick();
        Ok(self.queued as i64)
    }
    fn in_xrun(&mut self) -> Result<bool, SinkError> {
        Ok(false)
    }
    fn drain(&mut self) -> Result<(), SinkError> {
        self.tick();
        self.played += self.queued as u64;
        self.queued = 0.0;
        Ok(())
    }
    fn frames_played(&mut self) -> Result<u64, SinkError> {
        self.tick();
        Ok(self.played)
    }
}

// ----- sample helpers -------------------------------------------------------

fn encode_sample(format: SampleFormat, v: f64) -> Vec<u8> {
    match format {
        SampleFormat::PcmS16Le => (v as i16).to_le_bytes().to_vec(),
        SampleFormat::PcmS24Le => (v as i32).to_le_bytes()[..3].to_vec(),
        SampleFormat::PcmF32Le => (v as f32).to_le_bytes().to_vec(),
    }
}

fn decode_sample(format: SampleFormat, b: &[u8]) -> f64 {
    match format {
        SampleFormat::PcmS16Le => f64::from(i16::from_le_bytes([b[0], b[1]])),
        SampleFormat::PcmS24Le => f64::from(i32::from_le_bytes([0, b[0], b[1], b[2]]) >> 8),
        SampleFormat::PcmF32Le => f64::from(f32::from_le_bytes([b[0], b[1], b[2], b[3]])),
    }
}

/// Interleave per-channel sample values into one PCM buffer.
fn interleave(format: SampleFormat, channels: &[Vec<f64>]) -> Vec<u8> {
    let frames = channels[0].len();
    let mut out = Vec::new();
    for f in 0..frames {
        for ch in channels {
            out.extend(encode_sample(format, ch[f]));
        }
    }
    out
}

/// The device tape as one vector of values per device channel.
fn deinterleave(format: SampleFormat, channels: usize, pcm: &[u8]) -> Vec<Vec<f64>> {
    let bps = format.bytes_per_sample();
    let mut out = vec![Vec::new(); channels];
    for (i, s) in pcm.chunks_exact(bps).enumerate() {
        out[i % channels].push(decode_sample(format, s));
    }
    out
}

/// The same, as raw sample bytes, for bit-exact comparisons.
fn deinterleave_bytes(format: SampleFormat, channels: usize, pcm: &[u8]) -> Vec<Vec<u8>> {
    let bps = format.bytes_per_sample();
    let mut out = vec![Vec::new(); channels];
    for (i, s) in pcm.chunks_exact(bps).enumerate() {
        out[i % channels].extend_from_slice(s);
    }
    out
}

fn full_scale(format: SampleFormat) -> f64 {
    match format {
        SampleFormat::PcmS16Le => 32_767.0,
        SampleFormat::PcmS24Le => 8_388_607.0,
        SampleFormat::PcmF32Le => 1.0,
    }
}

/// Distinct per-channel signals: channel `c` is a ramp at its own level, so
/// no two channels ever carry the same value and a swapped channel is visible.
fn distinct_signals(format: SampleFormat, channels: usize, frames: usize) -> Vec<Vec<f64>> {
    let fs = full_scale(format);
    (0..channels)
        .map(|c| {
            (0..frames)
                .map(|f| {
                    let level = (c as f64 + 1.0) / 10.0; // 0.1 .. 0.6 of full scale
                    let wobble = ((f % 50) as f64 - 25.0) / 1_000.0;
                    let v = (level + wobble) * fs * if f % 2 == 0 { 1.0 } else { -1.0 };
                    if format == SampleFormat::PcmF32Le {
                        // What the wire can carry, so the expected values
                        // below start from the samples actually sent.
                        f64::from(v as f32)
                    } else {
                        v.round()
                    }
                })
                .collect()
        })
        .collect()
}

fn map(channels: &str, outputs: &[&str]) -> OutputMap {
    OutputMap::from_args(
        Some(channels),
        &outputs.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
    )
    .unwrap_or_else(|e| panic!("the map {:?} is valid: {}", outputs, e))
    .expect("a map was configured")
}

/// Push `pcm` through a mapped N-channel device in writes of `chunk_frames`
/// stream frames, and give back the device's tape.
fn play(
    map: &OutputMap,
    stream: &[ChannelPosition],
    format: SampleFormat,
    pcm: &[u8],
    chunk_frames: usize,
) -> (Vec<u8>, Vec<String>, u64) {
    let device = RecordingDevice::new(usize::from(map.channels()), format);
    let tape = device.tape();
    let (mut sink, resolved) =
        MappedSink::new(device, map, stream, format).expect("the map fits the device");
    let in_frame = stream.len() * format.bytes_per_sample();
    assert_eq!(sink.frame_len(), in_frame, "the sink takes stream frames");
    let mut frames_in = 0u64;
    let mut frames_out = 0u64;
    for piece in pcm.chunks(chunk_frames * in_frame) {
        let report = sink.write(piece).expect("the write lands");
        frames_in += (piece.len() / in_frame) as u64;
        frames_out += report.frames_written;
    }
    assert_eq!(
        frames_in, frames_out,
        "frames in are frames out, one for one"
    );
    let tape = tape.lock().unwrap().clone();
    (tape, resolved.report().to_vec(), sink.clipped_samples())
}

const ALL_FORMATS: [SampleFormat; 3] = [
    SampleFormat::PcmS16Le,
    SampleFormat::PcmS24Le,
    SampleFormat::PcmF32Le,
];

// ----- the tests ------------------------------------------------------------

#[test]
fn a_5_1_stream_lands_on_an_8_channel_device_in_alsa_order_with_two_extra_outputs() {
    // ALSA surround51 order FL FR RL RR FC LFE, then a stereo downmix line out
    // on 6 and nothing on 7 (unmapped).
    let m = map(
        "8",
        &["0=FL", "1=FR", "2=SL", "3=SR", "4=FC", "5=LFE", "6=FL+FR"],
    );
    for format in ALL_FORMATS {
        let frames = 960 * 3;
        let signals = distinct_signals(format, 6, frames);
        let pcm = interleave(format, &signals);
        let (tape, report, clipped) = play(&m, &FIVE_ONE, format, &pcm, 960);
        assert_eq!(tape.len(), frames * 8 * format.bytes_per_sample());
        let out = deinterleave_bytes(format, 8, &tape);
        let src = deinterleave_bytes(format, 6, &pcm);
        // Device channel <- stream channel, byte for byte.
        for (dev, stream) in [(0usize, 0usize), (1, 1), (2, 4), (3, 5), (4, 2), (5, 3)] {
            assert_eq!(
                out[dev],
                src[stream],
                "{}: device channel {} is stream channel {} bit for bit",
                format.name(),
                dev,
                stream
            );
        }
        // The downmix: (FL + FR) / 2, rounded to the nearest sample.
        let vals = deinterleave(format, 8, &tape);
        for f in 0..frames {
            let exact = (signals[0][f] + signals[1][f]) / 2.0;
            let expected = if format == SampleFormat::PcmF32Le {
                f64::from(exact as f32)
            } else {
                exact.round()
            };
            assert_eq!(
                vals[6][f],
                expected,
                "{} downmix frame {}",
                format.name(),
                f
            );
        }
        // The unmapped output is silence, every sample.
        assert!(vals[7].iter().all(|v| *v == 0.0), "{}", format.name());
        assert!(tape_bytes_zero(&out[7]));
        assert_eq!(clipped, 0);
        assert!(report
            .iter()
            .any(|l| l.contains("out=0 source=FL") && l.contains("exact=1")));
        assert!(report.iter().any(|l| l.contains("out=7 source=silence")));
        assert!(
            !report.iter().any(|l| l.contains("output-map-missing")),
            "nothing is missing from a 5.1 stream: {:?}",
            report
        );
        println!(
            "{}: 5.1 onto 8 channels: {}",
            format.name(),
            report.join(" | ")
        );
    }
}

fn tape_bytes_zero(b: &[u8]) -> bool {
    b.iter().all(|x| *x == 0)
}

#[test]
fn per_channel_gains_are_applied_numerically_in_every_format() {
    // One stream channel, FL, onto five outputs at five gains.
    let gains = [0.0, -3.0, -6.0, MIN_GAIN_DB, MAX_GAIN_DB];
    let outputs: Vec<String> = gains
        .iter()
        .enumerate()
        .map(|(i, g)| format!("{}=FL,gain-db={}", i, g))
        .collect();
    let outputs_ref: Vec<&str> = outputs.iter().map(String::as_str).collect();
    let m = map("5", &outputs_ref);
    for format in ALL_FORMATS {
        let fs = full_scale(format);
        // A quarter of full scale, so +6 dB still fits.
        let level = if format == SampleFormat::PcmF32Le {
            0.25
        } else {
            (fs / 4.0).round()
        };
        let signals = vec![vec![level; 480], vec![-level; 480]];
        let pcm = interleave(format, &signals);
        let (tape, _, clipped) = play(&m, &[FrontLeft, FrontRight], format, &pcm, 480);
        let vals = deinterleave(format, 5, &tape);
        for (i, g) in gains.iter().enumerate() {
            let exact = level * 10f64.powf(g / 20.0);
            let expected = if format == SampleFormat::PcmF32Le {
                f64::from(exact as f32)
            } else {
                exact.round()
            };
            assert!(
                vals[i].iter().all(|v| *v == expected),
                "{} at {} dB: expected {} and the device got {}",
                format.name(),
                g,
                expected,
                vals[i][0]
            );
            println!(
                "{} gain {:+} dB: {} -> {} (factor {:.6})",
                format.name(),
                g,
                level,
                vals[i][0],
                vals[i][0] / level
            );
        }
        assert_eq!(clipped, 0);
    }
}

#[test]
fn a_boost_saturates_at_full_scale_and_is_counted_never_wrapped() {
    let m = map("2", &["0=FL,gain-db=6", "1=FR,gain-db=6"]);
    for format in ALL_FORMATS {
        let fs = full_scale(format);
        let low = if format == SampleFormat::PcmF32Le {
            -1.0
        } else {
            -fs - 1.0
        };
        let signals = vec![vec![fs; 10], vec![low; 10]];
        let pcm = interleave(format, &signals);
        let (tape, _, clipped) = play(&m, &[FrontLeft, FrontRight], format, &pcm, 10);
        let vals = deinterleave(format, 2, &tape);
        assert!(vals[0].iter().all(|v| *v == fs), "{}", format.name());
        assert!(vals[1].iter().all(|v| *v == low), "{}", format.name());
        assert_eq!(
            clipped,
            20,
            "{}: every boosted full-scale sample saturated",
            format.name()
        );
    }
}

#[test]
fn a_per_channel_delay_moves_an_impulse_by_exactly_k_frames() {
    // delay-us=1000 is 48 frames at 48 kHz; 21 us rounds to 1 frame; the
    // bound, 50 ms, is 2400 frames.
    let delays_us = [0u64, 21, 1_000, 12_345, MAX_DELAY_US];
    let outputs: Vec<String> = delays_us
        .iter()
        .enumerate()
        .map(|(i, d)| format!("{}=FL,delay-us={}", i, d))
        .collect();
    let outputs_ref: Vec<&str> = outputs.iter().map(String::as_str).collect();
    let m = map("5", &outputs_ref);
    let expected_k: Vec<usize> = delays_us
        .iter()
        .map(|d| delay_frames(*d, RATE_HZ))
        .collect();
    assert_eq!(expected_k, vec![0, 1, 48, 593, 2_400]);
    for format in ALL_FORMATS {
        let frames = 4_000;
        let at = 100;
        let peak = full_scale(format) / 2.0;
        let mut fl = vec![0.0; frames];
        fl[at] = peak.round();
        let signals = vec![fl];
        let pcm = interleave(format, &signals);
        // Odd write sizes, so the delay line is carried across writes.
        let (tape, report, _) = play(&m, &[Mono], format, &pcm, 37);
        let vals = deinterleave(format, 5, &tape);
        for (c, k) in expected_k.iter().enumerate() {
            let hits: Vec<usize> = vals[c]
                .iter()
                .enumerate()
                .filter(|(_, v)| **v != 0.0)
                .map(|(i, _)| i)
                .collect();
            assert_eq!(
                hits,
                vec![at + k],
                "{}: output {} (delay {} frames) has its impulse at {:?}",
                format.name(),
                c,
                k,
                hits
            );
            assert_eq!(vals[c][at + k], peak.round());
        }
        println!(
            "{}: impulse at frame {} moved to {:?} by delays {:?} us",
            format.name(),
            at,
            expected_k.iter().map(|k| at + k).collect::<Vec<_>>(),
            delays_us
        );
        assert!(report.iter().any(|l| l.contains("delay_frames=2400")));
        // The mono stream fed FL through the documented fallback, and said so.
        assert!(report.iter().any(|l| l.starts_with("output-map-fallback")));
    }
}

#[test]
fn every_format_passes_through_bit_exact_at_0_db_with_no_delay() {
    // Awkward values on purpose: both extremes, -1, 0, a negative zero, a NaN
    // payload and a subnormal. A byte copy keeps every one of them.
    let m = map("2", &["0=FR", "1=FL"]);
    let raw: Vec<(SampleFormat, Vec<u8>)> = vec![
        (
            SampleFormat::PcmS16Le,
            [i16::MIN, i16::MAX, -1, 0, 1, 12_345]
                .iter()
                .flat_map(|v| v.to_le_bytes())
                .collect(),
        ),
        (
            SampleFormat::PcmS24Le,
            vec![
                0x00, 0x00, 0x80, 0xFF, 0xFF, 0x7F, 0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x00, 0x01, 0x00,
                0x00, 0x39, 0x30, 0x01,
            ],
        ),
        (
            SampleFormat::PcmF32Le,
            [
                f32::from_bits(0x7FC0_1234),
                -0.0f32,
                1.0,
                -1.0,
                f32::from_bits(1),
                0.123_456_79,
            ]
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect(),
        ),
    ];
    for (format, pcm) in raw {
        let (tape, report, _) = play(&m, &[FrontLeft, FrontRight], format, &pcm, 1);
        let bps = format.bytes_per_sample();
        let mut swapped = Vec::new();
        for frame in pcm.chunks_exact(2 * bps) {
            swapped.extend_from_slice(&frame[bps..]);
            swapped.extend_from_slice(&frame[..bps]);
        }
        assert_eq!(tape, swapped, "{}: a reorder is a byte copy", format.name());
        assert!(report
            .iter()
            .all(|l| !l.starts_with("output-map ") || l.contains("exact=1")));
    }
}

#[test]
fn a_downmix_sums_its_positions_with_equal_weight() {
    let m = map(
        "3",
        &[
            "0=FL+FR",
            "1=FL+FR+FC+LFE+SL+SR,gain-db=-3",
            "2=SL+SR,delay-us=1000",
        ],
    );
    for format in ALL_FORMATS {
        let signals = distinct_signals(format, 6, 960);
        let pcm = interleave(format, &signals);
        let (tape, _, clipped) = play(&m, &FIVE_ONE, format, &pcm, 960);
        let vals = deinterleave(format, 3, &tape);
        let round = |x: f64| {
            if format == SampleFormat::PcmF32Le {
                f64::from(x as f32)
            } else {
                x.round()
            }
        };
        for f in 0..960 {
            let s = |c: usize| signals[c][f];
            assert_eq!(vals[0][f], round((s(0) + s(1)) / 2.0));
            let all: f64 = (0..6).map(s).sum::<f64>() / 6.0 * 10f64.powf(-3.0 / 20.0);
            assert_eq!(vals[1][f], round(all));
            let side = if f >= 48 {
                round((signals[4][f - 48] + signals[5][f - 48]) / 2.0)
            } else {
                0.0
            };
            assert_eq!(vals[2][f], side, "{} frame {}", format.name(), f);
        }
        assert_eq!(clipped, 0);
    }
}

#[test]
fn a_stereo_stream_on_a_rack_amp_zone_map() {
    // A rack amp card of 8 outputs (ASSUMED layout, the rack's zones are an
    // owner input): zone A's pair on 0-1, a line out to an AVR on 2-3 trimmed
    // 6 dB down, a sub out on 4 fed from FL+FR, 2 ms late and boosted to undo
    // the downmix, and 5-7 unused.
    let m = map(
        "8",
        &[
            "0=FL",
            "1=FR",
            "2=FL,gain-db=-6",
            "3=FR,gain-db=-6",
            "4=FL+FR,gain-db=6,delay-us=2000",
        ],
    );
    assert_eq!(m.max_stream_channels(), 2, "the map reads two positions");
    let format = SampleFormat::PcmS16Le;
    let signals = distinct_signals(format, 2, 960 * 2);
    let pcm = interleave(format, &signals);
    let (tape, report, clipped) = play(&m, &[FrontLeft, FrontRight], format, &pcm, 960);
    let vals = deinterleave(format, 8, &tape);
    let k = delay_frames(2_000, RATE_HZ);
    assert_eq!(k, 96);
    let g6 = 10f64.powf(-6.0 / 20.0);
    for f in 0..960 * 2 {
        assert_eq!(vals[0][f], signals[0][f]);
        assert_eq!(vals[1][f], signals[1][f]);
        assert_eq!(vals[2][f], (signals[0][f] * g6).round());
        assert_eq!(vals[3][f], (signals[1][f] * g6).round());
        let sub = if f >= k {
            ((signals[0][f - k] + signals[1][f - k]) / 2.0 * 10f64.powf(6.0 / 20.0)).round()
        } else {
            0.0
        };
        assert_eq!(vals[4][f], sub, "sub frame {}", f);
        for c in 5..8 {
            assert_eq!(vals[c][f], 0.0);
        }
    }
    assert_eq!(clipped, 0);
    println!("rack amp zone map: {}", report.join(" | "));
}

#[test]
fn positions_the_stream_lacks_are_silence_and_are_reported() {
    // A 5.1 map meeting a stereo stream: FC, LFE, SL and SR are not there.
    let m = map("6", &["0=FL", "1=FR", "2=FC", "3=LFE", "4=SL", "5=FL+SR"]);
    let r = m.resolve(&[FrontLeft, FrontRight], SampleFormat::PcmS16Le, RATE_HZ);
    assert_eq!(
        r.missing(),
        &[
            (2, FrontCenter),
            (3, LowFrequency),
            (4, SideLeft),
            (5, SideRight)
        ]
    );
    let lines = r.report().join("\n");
    assert!(
        lines.contains("output-map-missing out=2 source=FC stream_lacks=FC stream=FL,FR silent=1")
    );
    assert!(lines
        .contains("output-map-missing out=5 source=FL+SR stream_lacks=SR stream=FL,FR silent=0"));
    println!("{}", lines);

    let signals = distinct_signals(SampleFormat::PcmS16Le, 2, 100);
    let pcm = interleave(SampleFormat::PcmS16Le, &signals);
    let (tape, _, _) = play(
        &m,
        &[FrontLeft, FrontRight],
        SampleFormat::PcmS16Le,
        &pcm,
        100,
    );
    let vals = deinterleave(SampleFormat::PcmS16Le, 6, &tape);
    for c in 2..5 {
        assert!(vals[c].iter().all(|v| *v == 0.0), "output {} is silent", c);
    }
    // A downmix with a missing position keeps its configured weight: FL/2.
    for f in 0..100 {
        assert_eq!(vals[5][f], (signals[0][f] / 2.0).round());
    }

    // And the other way round: stream channels no output reads are reported.
    let m = map("2", &["0=FL", "1=FR"]);
    let r = m.resolve(&FIVE_ONE, SampleFormat::PcmS16Le, RATE_HZ);
    assert_eq!(
        r.unused(),
        &[FrontCenter, LowFrequency, SideLeft, SideRight]
    );
    assert!(r.report().iter().any(|l| l.contains("unused=FC+LFE+SL+SR")));
}

#[test]
fn the_endpoint_advertises_max_channels_from_its_map_and_eight_without_one() {
    let default = ClientConfig::default();
    assert_eq!(default.output_map, None);
    assert_eq!(capabilities(&default).max_channels, 8);

    let args =
        |a: &[&str]| ClientConfig::from_args(a.iter().map(|s| s.to_string())).map(|(c, _)| c);
    let c = args(&[
        "--output-channels",
        "8",
        "--output",
        "0=FL",
        "--output",
        "1=FR",
        "--output",
        "2=SL",
        "--output",
        "3=SR",
        "--output",
        "4=FC",
        "--output",
        "5=LFE",
        "--output",
        "6=FL+FR",
    ])
    .expect("a 5.1 map parses");
    c.validate()
        .expect("and the rest of the configuration stands");
    assert_eq!(c.output_map.as_ref().unwrap().channels(), 8);
    assert_eq!(capabilities(&c).max_channels, 6);
    let c = args(&[
        "--output-channels",
        "4",
        "--output",
        "0=FL",
        "--output",
        "1=FR",
    ])
    .unwrap();
    assert_eq!(capabilities(&c).max_channels, 2);
}

#[test]
fn a_map_is_refused_with_an_error_that_names_the_fix() {
    let refuse = |channels: Option<&str>, outputs: &[&str]| -> MapError {
        OutputMap::from_args(
            channels,
            &outputs.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
        )
        .expect_err("the map is refused")
    };
    let cases: Vec<(MapError, &str)> = vec![
        (refuse(None, &["0=FL"]), "--output needs --output-channels"),
        (refuse(Some("2"), &[]), "needs at least one --output"),
        (refuse(Some("0"), &["0=FL"]), "1 to 8"),
        (refuse(Some("9"), &["0=FL"]), "1 to 8"),
        (refuse(Some("two"), &["0=FL"]), "1 to 8"),
        (refuse(Some("2"), &["FL"]), "<index>=<source>"),
        (refuse(Some("2"), &["x=FL"]), "device channel index"),
        (refuse(Some("2"), &["2=FL"]), "indices run 0 to 1"),
        (
            refuse(Some("2"), &["0=FL", "0=FR"]),
            "give each channel once",
        ),
        (refuse(Some("2"), &["0=LEFT"]), "use one of MONO FL FR"),
        (refuse(Some("2"), &["0=FL+FL"]), "names FL twice"),
        (refuse(Some("2"), &["0=MONO+FL"]), "MONO only on its own"),
        (
            refuse(Some("2"), &["0=FL,gain-db=6.5"]),
            "between -60 and +6 dB",
        ),
        (refuse(Some("2"), &["0=FL,gain-db=-61"]), "use silence"),
        (
            refuse(Some("2"), &["0=FL,gain-db=NaN"]),
            "between -60 and +6 dB",
        ),
        (
            refuse(Some("2"), &["0=FL,delay-us=50001"]),
            "from 0 to 50000",
        ),
        (refuse(Some("2"), &["0=FL,delay-us=-1"]), "from 0 to 50000"),
        (
            refuse(Some("2"), &["0=FL,delay-ms=1"]),
            "settings are gain-db and delay-us",
        ),
        (
            refuse(Some("2"), &["0=FL,gain-db=1,gain-db=2"]),
            "one setting twice",
        ),
        (
            refuse(Some("2"), &["0=silence", "1=silence"]),
            "every output of the map is silence",
        ),
    ];
    for (e, fix) in &cases {
        let text = e.to_string();
        assert!(
            text.contains(fix),
            "{:?} says {:?}, which does not name {:?}",
            e,
            text,
            fix
        );
        println!("refused: {}", text);
    }
    // Through the client's own command line, the refusal is a configuration
    // error (exit 2), never a default.
    let err = ClientConfig::from_args(
        ["--output-channels", "2", "--output", "0=FL,gain-db=12"]
            .iter()
            .map(|s| s.to_string()),
    )
    .unwrap_err();
    assert!(
        matches!(err, ConfigError::OutputMap(MapError::Gain { .. })),
        "{:?}",
        err
    );
    assert!(
        parse_output("3=fl").is_ok(),
        "names are read case-insensitively"
    );
}

#[test]
fn a_device_opened_with_another_channel_count_is_refused() {
    let m = map("8", &["0=FL", "1=FR"]);
    let device = RecordingDevice::new(2, SampleFormat::PcmS16Le);
    let err = MappedSink::new(device, &m, &[FrontLeft, FrontRight], SampleFormat::PcmS16Le)
        .err()
        .expect("a 2-channel device cannot carry an 8-channel map");
    assert!(err.to_string().contains("--output-channels"), "{}", err);
}

// ----- through the client's own playout path --------------------------------

fn chunk(sequence: u32, pcm: &[u8], channels: u16) -> Vec<u8> {
    encode(&Message::AudioChunk(AudioChunk {
        sequence,
        timestamp_ns: u64::from(sequence) * 20_000_000,
        sample_rate_hz: RATE_HZ,
        channels,
        sample_format: SampleFormat::PcmS16Le,
        reserved: [0u8; RESERVED_LEN],
        audio_data: pcm.to_vec(),
    }))
    .unwrap()
}

#[test]
fn the_playout_loop_plays_5_1_through_the_map_after_the_zone_gain() {
    // Constant per-channel values, so every device sample of the run has one
    // expected value; the zone is at 0.500, so what reaches the map is half
    // the stream, and the map's own gain and delay act on that.
    let values: [i16; 6] = [4_000, -6_000, 8_000, -10_000, 12_000, -14_000];
    let frame: Vec<u8> = values.iter().flat_map(|v| v.to_le_bytes()).collect();
    let pcm: Vec<u8> = frame.repeat(960);
    let chunks = 30u32;
    let mut parts: Vec<(Duration, Vec<u8>)> = (0..chunks)
        .map(|s| (Duration::from_millis(20 * u64::from(s)), chunk(s, &pcm, 6)))
        .collect();
    parts.push((
        Duration::from_millis(20 * u64::from(chunks)),
        encode(&Message::StreamEnd(StreamEnd {
            final_sequence: chunks - 1,
            end_timestamp_ns: u64::from(chunks) * 20_000_000,
        }))
        .unwrap(),
    ));
    let source = PacedReader::new(parts, true);

    let log_path =
        std::env::temp_dir().join(format!("chorus-multichannel-{}.log", std::process::id()));
    let config = ClientConfig {
        device: "modelled-n-channel".to_string(),
        delay_log: log_path.to_string_lossy().into_owned(),
        run_seconds: Some(4),
        output_map: Some(map(
            "8",
            &[
                "0=FL",
                "1=FR",
                "2=SL,gain-db=-6",
                "3=SR",
                "4=FC,delay-us=1000",
                "5=LFE",
                "6=FL+FR",
            ],
        )),
        ..Default::default()
    };
    config.validate().unwrap();
    let shape = StreamShape {
        sample_rate_hz: RATE_HZ,
        channels: 6,
        sample_format: SampleFormat::PcmS16Le,
        frames_per_chunk: 960,
    };
    let handshake = Handshake {
        receiver: fresh_receiver(),
        shape,
        buffered: Vec::new(),
    };
    let device = RecordingDevice::new(8, SampleFormat::PcmS16Le);
    let tape = device.tape();
    let (mut sink, resolved) = MappedSink::new(
        device,
        config.output_map.as_ref().unwrap(),
        &FIVE_ONE,
        SampleFormat::PcmS16Le,
    )
    .unwrap();
    let header = header_for(&config, "modelled-n-channel", &shape);
    let mut log = DelayLog::open(&log_path, &header).unwrap();
    let watch = Arc::new(ZoneWatch::new());
    assert!(watch.absorb(
        r#"{"v":1,"t":"state","serial":1,"zones":[{"id":"kitchen","name":"Kitchen","group":"g","volume":0.500,"muted":false,"endpoints":["a"],"present":["a"],"audio":"127.0.0.1:4010"}]}"#,
        "kitchen"
    ));
    let outcome = run_session(
        &config,
        source,
        handshake,
        &mut sink,
        &mut log,
        MonotonicTimeline::new(),
        Arc::new(Counters::new()),
        None,
        watch,
    )
    .unwrap();
    drop(log);
    let _ = std::fs::remove_file(&log_path);
    assert!(
        matches!(outcome.stop, StopReason::EndOfStream(_)),
        "{:?}",
        outcome.stop
    );

    let tape = tape.lock().unwrap().clone();
    let vals = deinterleave(SampleFormat::PcmS16Le, 8, &tape);
    let frames = vals[0].len();
    assert_eq!(
        frames,
        960 * chunks as usize,
        "every stream frame became one device frame"
    );
    let half = |v: i16| f64::from(v / 2);
    let g6 = 10f64.powf(-6.0 / 20.0);
    let k = delay_frames(1_000, RATE_HZ);
    for f in 0..frames {
        assert_eq!(vals[0][f], half(values[0]));
        assert_eq!(vals[1][f], half(values[1]));
        assert_eq!(vals[2][f], (half(values[4]) * g6).round());
        assert_eq!(vals[3][f], half(values[5]));
        assert_eq!(
            vals[4][f],
            if f < k { 0.0 } else { half(values[2]) },
            "frame {}",
            f
        );
        assert_eq!(vals[5][f], half(values[3]));
        assert_eq!(
            vals[6][f],
            ((half(values[0]) + half(values[1])) / 2.0).round()
        );
        assert_eq!(vals[7][f], 0.0);
    }
    println!(
        "playout 5.1 -> 8: {} frames, stop {}; {}",
        frames,
        outcome.stop.name(),
        resolved.report().join(" | ")
    );
}
