//! AC-1, the half that is read off the samples.
//!
//! "WHEN a zone is grouped, ungrouped, volume-changed or muted THE SYSTEM SHALL
//! apply it to every affected endpoint ... Applied-at-the-endpoint is read off
//! the samples the modelled sink accepted, not off any status line: after a
//! volume change the accepted PCM is scaled by the commanded factor within a
//! stated tolerance, after a mute it is all zero samples with frames still
//! flowing, and after a group change the endpoint is playing the chunks of the
//! stream its new group is aligned to."
//!
//! Every assertion below is on `RecordingSink::accepted`, which is the bytes a
//! device was handed. There is no report from the client that could satisfy
//! any of them, which is the point: `crates/client-linux/src/sink.rs` says the
//! shipped client "either plays through a real audio device or exits non-zero.
//! There is no 'pretend' sink behind a flag", and the modelled device here
//! lives under `tests/` where no binary can reach it.
//!
//! The group half is here too and is read the same way. A group change is the
//! server telling this endpoint that its zone's stream is at a different
//! address; the endpoint's session supervisor acts on that, and what this
//! grades is that the samples which then reach the device are the OTHER
//! stream's. `tools/control-plane-run.sh` runs the same thing with real
//! processes over real sockets.

mod common;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chorus_audio::MonotonicTimeline;
use chorus_client_linux::config::ClientConfig;
use chorus_client_linux::control::ZoneWatch;
use chorus_client_linux::delaylog::DelayLog;
use chorus_client_linux::receive::{Handshake, StreamShape};
use chorus_client_linux::run::{fresh_receiver, header_for, run_session};
use chorus_client_linux::sink::{PcmSink, SinkError, SinkWrite};
use chorus_client_linux::zone::SCALING_TOLERANCE_ULP;
use chorus_client_linux::Counters;
use chorus_control::catalog::Volume;
use chorus_protocol::SampleFormat;

use common::{end_frame, PacedReader};

const RATE_HZ: u32 = 48_000;
const FRAME_LEN: usize = 4;
const FRAMES_PER_CHUNK: usize = 960;

/// A modelled device that keeps every byte it was handed.
///
/// It has a ring, and the ring drains at the nominal rate, because the playout
/// loop paces its writes against the delay a device reports: a model that
/// answered with a constant would make the loop write everything in one drain
/// at the end, and a change made DURING a run would then never be a change made
/// during a run. Nothing else about it is modelled - it never underruns and
/// never refuses - because what is being graded here is the CONTENT of the
/// samples and `crates/client-linux/tests/playout.rs` already grades the rest
/// against a fuller model.
struct RecordingSink {
    accepted: Arc<Mutex<Vec<u8>>>,
    queued: f64,
    played: u64,
    last_tick: std::time::Instant,
}

impl RecordingSink {
    fn new() -> RecordingSink {
        RecordingSink {
            accepted: Arc::new(Mutex::new(Vec::new())),
            queued: 0.0,
            played: 0,
            last_tick: std::time::Instant::now(),
        }
    }

    fn tape(&self) -> Arc<Mutex<Vec<u8>>> {
        Arc::clone(&self.accepted)
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

impl PcmSink for RecordingSink {
    fn device(&self) -> &str {
        "modelled-recording"
    }

    fn frame_len(&self) -> usize {
        FRAME_LEN
    }

    fn rate_hz(&self) -> u32 {
        RATE_HZ
    }

    fn write(&mut self, pcm: &[u8]) -> Result<SinkWrite, SinkError> {
        self.tick();
        self.accepted
            .lock()
            .expect("the tape")
            .extend_from_slice(pcm);
        let frames = (pcm.len() / FRAME_LEN) as u64;
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

fn temp_path(name: &str) -> std::path::PathBuf {
    let mut dir = std::env::temp_dir();
    dir.push(format!(
        "chorus-zone-{}-{}-{}.log",
        name,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0)
    ));
    dir
}

fn a_config(log: &std::path::Path) -> ClientConfig {
    ClientConfig {
        server: "unused".to_string(),
        device: "modelled-recording".to_string(),
        delay_log: log.to_string_lossy().into_owned(),
        run_seconds: Some(4),
        ..Default::default()
    }
}

fn a_shape() -> StreamShape {
    StreamShape {
        sample_rate_hz: RATE_HZ,
        channels: 2,
        sample_format: SampleFormat::PcmS16Le,
        frames_per_chunk: FRAMES_PER_CHUNK as u64,
    }
}

/// A stream of `chunks` chunks whose every sample is `value`, then an end.
///
/// A constant-valued stream is what lets a scaling be read off the tape
/// exactly: every accepted sample of the run has one expected value, so a
/// single wrong chunk is visible rather than averaged away.
fn a_stream(chunks: u32, value: i16) -> (Handshake, PacedReader) {
    let mut parts: Vec<(Duration, Vec<u8>)> = Vec::new();
    let pcm: Vec<u8> = std::iter::repeat(value.to_le_bytes())
        .take(FRAMES_PER_CHUNK * 2)
        .flatten()
        .collect();
    for sequence in 0..chunks {
        let timestamp = u64::from(sequence) * 20_000_000;
        // Each part carries the time it is DUE, from the start of the run, so
        // these arrive one chunk duration apart, which is the rate a real
        // server emits at. Delivering them all at once would overflow the
        // client's own maximum occupancy and the run would be graded on the
        // chunks that survived that rather than on the ones that were sent.
        parts.push((
            Duration::from_millis(20 * u64::from(sequence)),
            chunk_frame_with(sequence, timestamp, &pcm),
        ));
    }
    parts.push((
        Duration::from_millis(20 * u64::from(chunks)),
        end_frame(chunks - 1, u64::from(chunks) * 20_000_000),
    ));
    let handshake = Handshake {
        receiver: fresh_receiver(),
        shape: a_shape(),
        buffered: Vec::new(),
    };
    (handshake, PacedReader::new(parts, true))
}

/// One audio chunk frame carrying exactly `pcm`.
fn chunk_frame_with(sequence: u32, timestamp_ns: u64, pcm: &[u8]) -> Vec<u8> {
    use chorus_protocol::{encode, AudioChunk, Message, RESERVED_LEN};
    encode(&Message::AudioChunk(AudioChunk {
        sequence,
        timestamp_ns,
        sample_rate_hz: RATE_HZ,
        channels: 2,
        sample_format: SampleFormat::PcmS16Le,
        reserved: [0u8; RESERVED_LEN],
        audio_data: pcm.to_vec(),
    }))
    .expect("the chunk encodes")
}

/// Run one session against a recording sink with `watch` in force, and hand
/// back every byte the device accepted.
fn accepted_with(watch: Arc<ZoneWatch>, chunks: u32, value: i16) -> Vec<i16> {
    let path = temp_path("apply");
    let config = a_config(&path);
    let (handshake, source) = a_stream(chunks, value);
    let mut sink = RecordingSink::new();
    let tape = sink.tape();
    let header = header_for(&config, "modelled-recording", &handshake.shape);
    let mut log = DelayLog::open(&path, &header).expect("the log opens");
    let counters = Arc::new(Counters::new());
    run_session(
        &config,
        source,
        handshake,
        &mut sink,
        &mut log,
        MonotonicTimeline::new(),
        counters,
        None,
        watch,
    )
    .expect("the run writes its log");
    drop(log);
    let _ = std::fs::remove_file(&path);
    let bytes = tape.lock().expect("the tape").clone();
    bytes
        .chunks_exact(2)
        .map(|c| i16::from_le_bytes([c[0], c[1]]))
        .collect()
}

/// A state message for one zone at a given volume and mute.
fn state(serial: u64, volume: &str, muted: bool, audio: &str) -> String {
    format!(
        r#"{{"v":1,"t":"state","serial":{},"zones":[{{"id":"kitchen","name":"Kitchen",\
"group":"downstairs","volume":{},"muted":{},"endpoints":["a"],"present":["a"],\
"audio":"{}"}}]}}"#,
        serial, volume, muted, audio
    )
    .replace("\\\n", "")
}

#[test]
fn with_no_control_channel_at_all_the_samples_are_the_ones_the_server_sent() {
    // The control is against the two below: an endpoint that has been told
    // nothing plays what it was sent, unchanged.
    let accepted = accepted_with(Arc::new(ZoneWatch::new()), 40, 12_000);
    assert!(!accepted.is_empty(), "the run played nothing at all");
    assert!(
        accepted.iter().all(|s| *s == 12_000),
        "an endpoint with no zone must not change a sample: {:?}",
        &accepted[..8.min(accepted.len())]
    );
}

#[test]
fn after_a_volume_change_the_accepted_pcm_is_scaled_by_the_commanded_factor() {
    for (literal, thousandths) in [("0.500", 500u32), ("0.375", 375), ("0.001", 1), ("1.000", 1000)]
    {
        let watch = Arc::new(ZoneWatch::new());
        assert!(
            watch.absorb(&state(1, literal, false, "127.0.0.1:4010"), "kitchen"),
            "the state message has to be about this zone"
        );
        assert_eq!(watch.gain(), Volume::from_thousandths(i64::from(thousandths)).unwrap());

        let source = 12_000i16;
        let accepted = accepted_with(watch, 40, source);
        assert!(!accepted.is_empty(), "the run played nothing at all");
        let exact = f64::from(source) * f64::from(thousandths) / 1_000.0;
        for sample in &accepted {
            assert!(
                (f64::from(*sample) - exact).abs() <= SCALING_TOLERANCE_ULP as f64,
                "at volume {} a source sample of {} reached the device as {}, against an exact \
                 {}",
                literal,
                source,
                sample,
                exact
            );
        }
    }
}

#[test]
fn after_a_mute_the_accepted_pcm_is_all_zero_and_frames_are_still_flowing() {
    let watch = Arc::new(ZoneWatch::new());
    watch.absorb(&state(1, "0.500", true, "127.0.0.1:4010"), "kitchen");
    assert_eq!(watch.gain(), Volume::SILENT);

    let muted = accepted_with(Arc::clone(&watch), 40, 12_000);
    assert!(
        muted.iter().all(|s| *s == 0),
        "a muted zone must hand the device silence"
    );

    // "with frames still flowing" is the load-bearing half: a mute that stopped
    // writing would change this endpoint's alignment and coming back from it
    // would be a resync. The same run unmuted has to write the same number of
    // frames.
    let unmuted_watch = Arc::new(ZoneWatch::new());
    unmuted_watch.absorb(&state(1, "0.500", false, "127.0.0.1:4010"), "kitchen");
    let unmuted = accepted_with(unmuted_watch, 40, 12_000);
    let chunk_samples = FRAMES_PER_CHUNK * 2;
    assert!(
        muted.len() >= 30 * chunk_samples,
        "a muted zone wrote only {} samples; frames have to keep flowing",
        muted.len()
    );
    assert_eq!(
        muted.len() % chunk_samples,
        0,
        "a mute must not write part of a chunk: {} samples is not a whole number of {}",
        muted.len(),
        chunk_samples
    );
    assert!(
        (muted.len() as i64 - unmuted.len() as i64).abs() <= chunk_samples as i64,
        "a mute wrote {} samples where an unmuted run wrote {}; the two runs are separate and \
         may differ by the chunk one of them was mid-way through, and by no more",
        muted.len(),
        unmuted.len()
    );
    assert!(unmuted.iter().any(|s| *s != 0), "the unmuted control is silent");
}

#[test]
fn unmuting_gives_back_the_volume_that_was_set_and_not_full_scale() {
    let watch = Arc::new(ZoneWatch::new());
    watch.absorb(&state(1, "0.250", false, "127.0.0.1:4010"), "kitchen");
    watch.absorb(&state(2, "0.250", true, "127.0.0.1:4010"), "kitchen");
    assert_eq!(watch.gain(), Volume::SILENT);
    watch.absorb(&state(3, "0.250", false, "127.0.0.1:4010"), "kitchen");
    let accepted = accepted_with(watch, 20, 8_000);
    for sample in &accepted {
        assert!(
            (f64::from(*sample) - 2_000.0).abs() <= SCALING_TOLERANCE_ULP as f64,
            "unmuting returned {} rather than the quarter volume that was set",
            sample
        );
    }
}

#[test]
fn after_a_group_change_the_endpoint_plays_the_chunks_of_its_new_groups_stream() {
    // Two streams, distinguishable by their samples: the endpoint's zone is on
    // the first, then the server puts it in a group whose stream is the second,
    // and what the device then accepts is the second stream's samples.
    let watch = Arc::new(ZoneWatch::new());
    watch.absorb(&state(1, "1.000", false, "127.0.0.1:4010"), "kitchen");
    assert_eq!(watch.facts().audio, "127.0.0.1:4010");
    assert_eq!(watch.moves(), 0);

    let first = accepted_with(Arc::clone(&watch), 20, 4_000);
    assert!(first.iter().all(|s| *s == 4_000));

    // The group change, as the server would send it.
    watch.absorb(
        &state(2, "1.000", false, "127.0.0.1:4011").replace(
            r#""group":"downstairs""#,
            r#""group":"upstairs""#,
        ),
        "kitchen",
    );
    assert_eq!(watch.facts().group, "upstairs");
    assert_eq!(
        watch.moves(),
        1,
        "the endpoint has to notice that its stream moved; that is what makes it reconnect"
    );
    assert_eq!(watch.facts().audio, "127.0.0.1:4011");

    // The session the supervisor then starts is against the other stream.
    let second = accepted_with(watch, 20, 9_000);
    assert!(
        second.iter().all(|s| *s == 9_000),
        "after the group change the device has to be accepting the OTHER stream's chunks"
    );
    assert_ne!(first[0], second[0]);
}

#[test]
fn a_volume_change_mid_run_reaches_the_samples_without_the_session_restarting() {
    // The two above set the gain before the run. This one changes it DURING
    // one, which is the case a status line could most easily be right about
    // while the samples were wrong.
    let path = temp_path("mid-run");
    let config = a_config(&path);
    let (handshake, source) = a_stream(120, 16_000);
    let mut sink = RecordingSink::new();
    let tape = sink.tape();
    let header = header_for(&config, "modelled-recording", &handshake.shape);
    let mut log = DelayLog::open(&path, &header).expect("the log opens");
    let counters = Arc::new(Counters::new());
    let watch = Arc::new(ZoneWatch::new());

    let changed = Arc::new(AtomicBool::new(false));
    let changer = {
        let watch = Arc::clone(&watch);
        let changed = Arc::clone(&changed);
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(400));
            watch.absorb(&state(1, "0.500", false, "127.0.0.1:4010"), "kitchen");
            changed.store(true, Ordering::SeqCst);
        })
    };

    run_session(
        &config,
        source,
        handshake,
        &mut sink,
        &mut log,
        MonotonicTimeline::new(),
        counters,
        None,
        Arc::clone(&watch),
    )
    .expect("the run writes its log");
    changer.join().expect("the changer finished");
    drop(log);
    let text = std::fs::read_to_string(&path).expect("the log is readable");
    let _ = std::fs::remove_file(&path);

    let bytes = tape.lock().expect("the tape").clone();
    let accepted: Vec<i16> = bytes
        .chunks_exact(2)
        .map(|c| i16::from_le_bytes([c[0], c[1]]))
        .collect();
    assert!(changed.load(Ordering::SeqCst));
    assert!(
        accepted.iter().any(|s| *s == 16_000),
        "the run has to have played at full scale before the change"
    );
    assert!(
        accepted.iter().any(|s| (*s - 8_000).abs() <= 1),
        "the run has to have played at half scale after it; what reached the device was {:?}",
        &accepted[accepted.len().saturating_sub(8)..]
    );
    // Every accepted sample is one of the two, so nothing in between was
    // written at some third value.
    for sample in &accepted {
        assert!(
            *sample == 16_000 || (*sample - 8_000).abs() <= 1,
            "a sample of {} reached the device and is neither the source nor the source halved",
            sample
        );
    }
    // And the log records it, so a run can be graded afterwards by someone who
    // did not watch it.
    assert!(
        text.contains("kind=zone-gain") && text.contains("gain=0.500"),
        "the delay log has no zone-gain event in it"
    );
}
