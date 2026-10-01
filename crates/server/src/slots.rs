//! Stream slots: every group's stream, cut by one thread on one grid.
//!
//! # What a slot is
//!
//! One stream this process serves: a [`Fanout`] the sessions of one group are
//! routed to (`crate::router`) and the INPUT it plays. `--slots S` fixes how
//! many there are for the life of the process, so the number of groups that
//! can play at once is a declared quantity rather than a function of how many
//! a person forms; a change that would need one more is refused by name
//! before it is applied (`crate::slot_table`).
//!
//! # One grid
//!
//! Every slot is cut on THIS thread, the one real-time audio thread, in
//! lock-step: tick `k` emits chunk `k` of every slot, with the same sequence
//! and the same presentation timestamp, `origin + k * chunk duration` on the
//! one [`MonotonicTimeline`] (the relation `chorus_audio::Chunker` stamps
//! with). That shared grid is what lets a session move between slots with no
//! restart: its sequences stay contiguous and only the content changes
//! (`crate::router`). The thread population does not depend on S: S slots
//! cost S broadcasts per tick, not S threads.
//!
//! # Inputs
//!
//! What a slot plays is switched at a chunk boundary, by a [`SlotCommand`]
//! drained with `try_recv` (at the top of each tick, and again under the grid
//! guard just before the broadcast), so the conductor that decides it never
//! blocks this thread and this thread never waits for it. The inputs are
//! [`SlotInput::Silence`], [`SlotInput::Stream`] (the configured `--source`,
//! read ONCE per tick however many slots play it), [`SlotInput::Chime`] (a
//! chime rendered at start, repeated with a gap) and [`SlotInput::LineIn`]
//! (an endpoint's line-in through its port, `crate::linein`, played through
//! the latency-growth plan of ADR 0071 so its latency grows without a glitch
//! when a room joins its group, K94).
//!
//! A line-in's chunks carry the grid's sequence and timestamp like every
//! other slot's: what its plan moves is which source frames a chunk carries,
//! so a session that moves into or out of its group sees no timestamp jump.
//!
//! The configured stream is handed to this thread over a channel, as the
//! one-stream shape hands it, and when it ends (a file's last byte) the
//! slots playing it play silence until the supervisor hands over the next
//! one. There is no `stream_end` in this shape: a slot does not end, its
//! input changes.
//!
//! # The visualizer
//!
//! After each tick's broadcast, every slot a visualizer session is on
//! ([`Router::watched`]) has the chunk it just played analysed by its own
//! `chorus_dsp::visualizer::Analyzer` (its spectrum from the channels'
//! mean, its peak from every channel), and each frame that completes goes
//! to those sessions through [`Router::push_visualizer`], stamped where the
//! analysis says on
//! the grid's timeline (`origin + sample * 1e9 / rate`) plus each session's
//! heard latency. The analysers are allocated before the first tick and a
//! slot nobody watches costs nothing; a slot watched again after a gap is
//! analysed from scratch (the analyser is reset at the new position). A run
//! of silent frames sends its first and then nothing until there is
//! something to show (`docs/visualizer.md`).
//!
//! No clock but the monotonic timeline is read here, and nothing here waits
//! on anything but the pace of the grid.

use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use chorus_audio::MonotonicTimeline;
use chorus_dsp::visualizer::{Analyzer, Frame};
use chorus_protocol::{encode, AudioChunk, Message, SampleFormat, RESERVED_LEN};
use chorus_sync::latency_grow::{
    CubicResampler, GrowthConfig, LatencyPlan, MAX_RATE_DEVIATION, RAMP_MS,
};

use crate::linein::{decode_sample, encode_sample, Port};
use crate::router::Router;
use crate::serve::{ServeError, ServeParams};
use crate::source::PcmSource;
use crate::stream::Outbound;

/// What a slot plays.
///
/// Inputs are values the audio thread owns once they are handed over: a
/// later input that carries a resource (a rendered chime, a line-in's
/// receiver) is handed over the same channel, and the one it replaces is
/// handed back rather than dropped here, so this thread never frees or closes
/// anything it did not allocate per tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotInput {
    /// Zeros, which are silence in every supported format.
    Silence,
    /// The configured `--source`, shared by every slot that plays it.
    Stream,
    /// A generated chime, by its index in `chorus_schedule::chime::CHIMES`,
    /// rendered at start ([`SlotMedia::chimes`]) and repeated with a
    /// [`CHIME_GAP_MS`] gap for as long as it is the slot's input.
    Chime(u8),
    /// An endpoint's line-in, by the port its upstream goes into
    /// (`crate::linein`), played through the latency plan.
    LineIn(u8),
}

impl SlotInput {
    /// The word a status line uses.
    pub fn name(self) -> &'static str {
        match self {
            SlotInput::Silence => "silence",
            SlotInput::Stream => "stream",
            SlotInput::Chime(_) => "chime",
            SlotInput::LineIn(_) => "line-in",
        }
    }
}

/// A change to one slot, applied at the next chunk boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SlotCommand {
    /// Play `input` on `slot` from the next tick.
    Input {
        /// The slot.
        slot: usize,
        /// What it plays.
        input: SlotInput,
    },
    /// The latency the line-in on `port` is to be played at, ns from its
    /// anchor (ADR 0071: L_local while the group is only the source's own
    /// room, the group's tier latency otherwise). The plan grows or shrinks
    /// to it without a glitch; it is kept for the slot's next line-in too.
    LatencyTarget {
        /// The line-in port (`crate::linein`) whose plan it is.
        port: usize,
        /// The latency, ns.
        latency_ns: i64,
    },
}

/// The gap between two plays of a chime while it is a slot's input, ms.
/// ASSUMED: two seconds, long enough to read as a repeated chime rather than
/// a drone, short enough that an alarm keeps sounding.
pub const CHIME_GAP_MS: u64 = 2_000;

/// The latency a line-in starts at: ADR 0071's L_local, ns. ASSUMED there
/// (a 20 ms chunk to fill, the wired path, server processing and an endpoint
/// guard). On the wire it is the stamp offset the line-in starts at: chunks
/// are stamped on the slots' one grid, so a session moving into or out of a
/// line-in's slot sees no timestamp jump, and what the plan moves is which
/// source frames each grid chunk carries.
pub const LOCAL_LATENCY_NS: i64 = 30_000_000;

/// Whole chunks a line-in's port must hold before its plan starts.
/// ASSUMED: two (40 ms at the default chunk): one to play and one of slack
/// against the upstream's arrival jitter. A tick that still finds too little
/// plays silence and the plan waits (the latency grows by that chunk and
/// the shortfall is counted), so a late upstream costs a gap, never a splice.
pub const LINE_IN_START_CHUNKS: usize = 2;

/// How many source frames a line-in's resampler holds at most, in ms.
/// ASSUMED: two seconds, above the start fill plus the largest growth
/// (L_local to the wireless tier's 500 ms) with room for jitter.
pub const LINE_IN_HOLD_MS: u64 = 2_000;

/// What the inputs that carry their own audio are made of, built before the
/// audio thread starts so that it allocates nothing for them.
#[derive(Debug, Clone, Default)]
pub struct SlotMedia {
    /// Each chime of `chorus_schedule::chime::CHIMES`, rendered at the
    /// server's format.
    pub chimes: Vec<Arc<[u8]>>,
    /// The line-in ports (`crate::linein`).
    pub ports: Vec<Arc<Port>>,
}

/// What a slot's line-in is doing, on the audio thread. One per slot,
/// allocated at start.
struct LineInPlayer {
    generation: u64,
    plan: Option<LatencyPlan>,
    resampler: CubicResampler,
    out: Vec<f64>,
    target_ns: i64,
}

/// What the audio thread counts about line-ins, for the run's last line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LineInCounts {
    /// Ticks a playing line-in had too little upstream for, after it started.
    pub underruns: u64,
    /// Line-in plans started (one per stream).
    pub starts: u64,
}

/// What the audio thread tells the supervisor.
#[derive(Debug)]
pub enum SlotEvent {
    /// The configured stream delivered its last byte. The slots playing it
    /// play silence until another is handed over.
    StreamEnded {
        /// Chunks cut from it.
        chunks: u64,
    },
}

/// What a run of the slots did, when it stops.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SlotsReport {
    /// Ticks emitted: one chunk on every slot each.
    pub ticks: u64,
    /// Sequence of the last tick, when there was one.
    pub final_sequence: u32,
    /// The line-ins.
    pub line_in: LineInCounts,
    /// The visualizer stream.
    pub visualizer: VisualizerCounts,
}

/// What the audio thread counts about the visualizer stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct VisualizerCounts {
    /// `visualizer_frame` messages queued, over every session.
    pub frames: u64,
    /// `color` messages queued.
    pub colours: u64,
    /// Messages a full session queue refused.
    pub dropped: u64,
    /// Frames analysed and not sent: silence after silence already sent.
    pub quiet: u64,
}

/// One slot's visualizer, on the audio thread: allocated at start.
struct SlotVisualizer {
    analyzer: Analyzer,
    samples: Vec<f32>,
    frames: Vec<Frame>,
    bands: Vec<u8>,
    quiet_sent: bool,
}

/// Serve every slot until `keep` says stop or the configured stream fails.
///
/// `router` holds the fanouts (slot by slot, then the silent one) and the grid
/// guard; `commands` are drained once per tick; `streams` delivers the
/// configured stream, and again after each [`SlotEvent::StreamEnded`].
#[allow(clippy::too_many_arguments)]
pub fn serve_slots(
    params: ServeParams,
    timeline: MonotonicTimeline,
    router: &Router,
    media: &SlotMedia,
    commands: &Receiver<SlotCommand>,
    streams: &Receiver<Box<dyn PcmSource>>,
    events: &Sender<SlotEvent>,
    keep: &AtomicBool,
) -> Result<SlotsReport, ServeError> {
    let slots = router.slots();
    let mut inputs = vec![SlotInput::Silence; slots];
    let frames = params
        .format
        .frames_in(params.chunk_us)
        .map_err(|e| ServeError::Encode(e.to_string()))?;
    let frame_len = params.format.frame_len();
    let bytes_per_chunk = frames * frame_len;
    let channels = usize::from(params.format.channels);
    let rate = u64::from(params.format.sample_rate_hz);
    let growth = GrowthConfig {
        sample_rate_hz: params.format.sample_rate_hz,
        chunk_frames: frames as u32,
        max_rate_deviation: MAX_RATE_DEVIATION,
        ramp_frames: RAMP_MS * rate / 1_000,
    };
    let line_ins_playable = growth.validate().is_ok();
    // Everything the chimes and line-ins need, allocated here, before the
    // first tick: each slot's chime position and PCM, and each line-in
    // port's player, with a resampler and an output buffer at full size.
    // A line-in's player belongs to its PORT, not to a slot: a group that
    // moves to another slot (a room joining it forms a new group) keeps
    // playing the same plan, so its latency grows rather than restarts.
    let mut slot_pcm = vec![vec![0u8; bytes_per_chunk]; slots];
    let mut chime_at = vec![0usize; slots];
    let gap_bytes = (CHIME_GAP_MS * rate / 1_000) as usize * frame_len;
    let hold_frames = (LINE_IN_HOLD_MS * rate / 1_000) as usize;
    let ports = media.ports.len();
    let mut players: Vec<LineInPlayer> = (0..ports)
        .map(|_| LineInPlayer {
            generation: u64::MAX,
            plan: None,
            resampler: CubicResampler::with_capacity(channels, hold_frames),
            out: Vec::with_capacity(frames * channels),
            target_ns: LOCAL_LATENCY_NS,
        })
        .collect();
    let mut port_pcm = vec![vec![0u8; bytes_per_chunk]; ports];
    // One analyser per slot, at this server's rate (`None` for a rate the
    // analysis does not take, which the wire never carries).
    let mut visualizers: Vec<Option<SlotVisualizer>> = (0..slots)
        .map(|_| {
            Analyzer::new(params.format.sample_rate_hz).map(|analyzer| SlotVisualizer {
                analyzer,
                samples: Vec::with_capacity(frames * channels),
                frames: Vec::with_capacity(frames / 100 + 8),
                bands: Vec::with_capacity(chorus_dsp::visualizer::BANDS),
                quiet_sent: false,
            })
        })
        .collect();
    let mut port_played = vec![false; ports];
    let chunk_ns = params.chunk_us * 1_000;
    let interval_ns = params.emit_interval_ns();
    let origin_ns = timeline.now_ns();
    let mut next_emit_ns = origin_ns;
    let mut stream: Option<Box<dyn PcmSource>> = None;
    let mut stream_chunks = 0u64;
    let mut pcm = vec![0u8; bytes_per_chunk];
    let silence = vec![0u8; bytes_per_chunk];
    let mut report = SlotsReport::default();
    let mut sequence: u32 = 0;

    while keep.load(Ordering::SeqCst) {
        // What each slot plays, as far as is known before the stream is read
        // (the guard below takes any command sent since).
        apply_commands(commands, &mut inputs, &mut chime_at, &mut players);
        if stream.is_none() {
            if let Ok(next) = streams.try_recv() {
                stream = Some(next);
                stream_chunks = 0;
            }
        }

        // Pace on the monotonic timeline, as the one-stream shape does.
        let now = timeline.now_ns();
        if next_emit_ns > now {
            let wait = next_emit_ns - now;
            if wait > 1_000 {
                thread::sleep(Duration::from_nanos(wait));
            }
        }
        next_emit_ns = next_emit_ns.saturating_add(interval_ns);

        // The configured stream is read once per tick, and only when a slot
        // plays it: a file nobody is listening to waits where it is. It is
        // read before the grid guard is taken (a FIFO may wait in a read).
        let mut have_stream = false;
        if inputs.contains(&SlotInput::Stream) {
            if let Some(source) = stream.as_mut() {
                match fill(source.as_mut(), &mut pcm) {
                    Ok(true) => {
                        have_stream = true;
                        stream_chunks += 1;
                    }
                    Ok(false) => {
                        // Its last byte: the partial chunk plays, padded with
                        // silence, and the supervisor is told.
                        have_stream = true;
                        stream = None;
                        let _ = events.send(SlotEvent::StreamEnded {
                            chunks: stream_chunks + 1,
                        });
                    }
                    Err(e) => return Err(ServeError::Source(e)),
                }
            }
        }

        let timestamp_ns = origin_ns.saturating_add(u64::from(sequence) * chunk_ns);
        let cut = |bytes: &[u8]| -> Result<Outbound, ServeError> {
            let chunk = AudioChunk {
                sequence,
                timestamp_ns,
                sample_rate_hz: params.format.sample_rate_hz,
                channels: params.format.channels,
                sample_format: params.format.sample_format,
                reserved: [0u8; RESERVED_LEN],
                audio_data: bytes.to_vec(),
            };
            encode(&Message::AudioChunk(chunk))
                .map(|f| Outbound::Frame(Arc::new(f)))
                .map_err(|e| ServeError::Encode(e.to_string()))
        };
        let quiet = cut(&silence)?;
        let playing = if have_stream { Some(cut(&pcm)?) } else { None };
        {
            // The grid guard is held from the commands to the broadcast, so a
            // session the conductor moves between two slots (under the same
            // guard) is moved either before this tick's inputs are settled or
            // after its chunks are out: never between the two, where it could
            // hear a slot whose input had not yet changed.
            let _one_tick = router.grid();
            // At the chunk boundary: what each slot plays from this tick on,
            // including any command that arrived while the stream was read.
            apply_commands(commands, &mut inputs, &mut chime_at, &mut players);
            // The inputs that carry their own audio, with no allocation but
            // the chunk's own: a chime into its slot's buffer, a line-in once
            // per port however many slots play it.
            port_played.fill(false);
            for slot in 0..slots {
                match inputs[slot] {
                    SlotInput::Chime(c) => match media.chimes.get(usize::from(c)) {
                        Some(chime) => {
                            chime_at[slot] =
                                play_chime(chime, gap_bytes, chime_at[slot], &mut slot_pcm[slot]);
                        }
                        None => slot_pcm[slot].fill(0),
                    },
                    SlotInput::LineIn(p) => {
                        let p = usize::from(p);
                        if let Some(port) = media.ports.get(p) {
                            if line_ins_playable && !port_played[p] {
                                play_line_in(
                                    &mut players[p],
                                    port,
                                    &growth,
                                    frames,
                                    params.format.sample_format,
                                    &mut port_pcm[p],
                                    &mut report.line_in,
                                );
                                port_played[p] = true;
                            }
                        }
                    }
                    SlotInput::Silence | SlotInput::Stream => {}
                }
            }
            for (slot, input) in inputs.iter().enumerate() {
                let frame = match (input, &playing) {
                    (SlotInput::Stream, Some(frame)) => frame.clone(),
                    (SlotInput::Chime(_), _) => cut(&slot_pcm[slot])?,
                    (SlotInput::LineIn(p), _)
                        if port_played.get(usize::from(*p)) == Some(&true) =>
                    {
                        cut(&port_pcm[usize::from(*p)])?
                    }
                    _ => quiet.clone(),
                };
                router.fanouts()[slot].broadcast(frame);
            }
            router.fanouts()[router.idle()].broadcast(quiet);
        }
        // The visualizer, outside the grid guard: what each watched slot
        // just played, analysed and sent to its visualizer sessions.
        for (slot, input) in inputs.iter().enumerate() {
            if !router.watched(slot) {
                continue;
            }
            let Some(v) = visualizers[slot].as_mut() else {
                continue;
            };
            let played: &[u8] = match input {
                SlotInput::Stream if have_stream => &pcm,
                SlotInput::Chime(_) => &slot_pcm[slot],
                SlotInput::LineIn(p) if port_played.get(usize::from(*p)) == Some(&true) => {
                    &port_pcm[usize::from(*p)]
                }
                _ => &silence,
            };
            let start = report.ticks * frames as u64;
            if v.analyzer.position() != start {
                v.analyzer.reset_at(start);
                v.quiet_sent = false;
            }
            let width = params.format.sample_format.bytes_per_sample();
            v.samples.clear();
            v.samples.extend(
                played
                    .chunks_exact(width)
                    .map(|b| decode_sample(b, params.format.sample_format) as f32),
            );
            v.frames.clear();
            v.analyzer.push(&v.samples, channels, &mut v.frames);
            for f in &v.frames {
                if f.is_silent() {
                    if v.quiet_sent {
                        report.visualizer.quiet += 1;
                        continue;
                    }
                    v.quiet_sent = true;
                } else {
                    v.quiet_sent = false;
                }
                let at_ns = origin_ns.saturating_add(
                    (u128::from(f.at_sample) * 1_000_000_000 / u128::from(rate)) as u64,
                );
                let pushed = router.push_visualizer(slot, at_ns, f, &mut v.bands);
                report.visualizer.frames += pushed.frames;
                report.visualizer.colours += pushed.colours;
                report.visualizer.dropped += pushed.dropped;
            }
        }
        report.ticks += 1;
        report.final_sequence = sequence;
        sequence = sequence.wrapping_add(1);
    }
    Ok(report)
}

/// Drain the slot commands waiting now, without blocking.
fn apply_commands(
    commands: &Receiver<SlotCommand>,
    inputs: &mut [SlotInput],
    chime_at: &mut [usize],
    players: &mut [LineInPlayer],
) {
    while let Ok(command) = commands.try_recv() {
        match command {
            SlotCommand::Input { slot, input } => {
                if let Some(at) = inputs.get_mut(slot) {
                    if *at != input {
                        *at = input;
                        chime_at[slot] = 0;
                    }
                }
            }
            SlotCommand::LatencyTarget { port, latency_ns } => {
                if let Some(p) = players.get_mut(port) {
                    p.target_ns = latency_ns;
                    if let Some(plan) = p.plan.as_mut() {
                        let _ = plan.set_target(latency_ns);
                    }
                }
            }
        }
    }
}

/// One chunk of a chime played on repeat: the rendered chime, then
/// `gap_bytes` of silence, from byte `at` of that cycle. Returns where the
/// next chunk starts.
fn play_chime(chime: &[u8], gap_bytes: usize, mut at: usize, out: &mut [u8]) -> usize {
    let cycle = chime.len() + gap_bytes;
    if cycle == 0 {
        out.fill(0);
        return 0;
    }
    let mut written = 0usize;
    while written < out.len() {
        at %= cycle;
        let n = if at < chime.len() {
            let n = (chime.len() - at).min(out.len() - written);
            out[written..written + n].copy_from_slice(&chime[at..at + n]);
            n
        } else {
            let n = (cycle - at).min(out.len() - written);
            out[written..written + n].fill(0);
            n
        };
        written += n;
        at += n;
    }
    at % cycle
}

/// One chunk of a line-in through its latency plan (ADR 0071), into `out`.
///
/// Whatever the port holds is moved into the resampler first (as much as it
/// has room for, so the resampler never grows). Until the start fill is in
/// hand the chunk is silence; then the plan starts at [`LOCAL_LATENCY_NS`]
/// heading for the slot's target, and every chunk is the plan's next one,
/// rendered by the cubic resampler. The plan's first chunk is two frames
/// short (its lookahead): its first frame is held for those two, so the
/// stream starts with no inserted zeros. A tick whose source frames have not
/// all arrived plays silence and the plan does NOT move on (it is a clone
/// that is asked first), so a late upstream costs a gap and a chunk of added
/// latency, never a dropped or repeated frame of the source.
fn play_line_in(
    player: &mut LineInPlayer,
    port: &Port,
    growth: &GrowthConfig,
    frames: usize,
    format: SampleFormat,
    out: &mut [u8],
    counts: &mut LineInCounts,
) {
    let channels = out.len() / frames / format.bytes_per_sample();
    let width = format.bytes_per_sample();
    if player.generation != port.generation() {
        player.generation = port.generation();
        player.plan = None;
        player.resampler.clear();
    }
    let room = player.resampler.room();
    let resampler = &mut player.resampler;
    let (_, generation) = port.drain(room, |samples| resampler.push(samples));
    if generation != player.generation {
        // Reset between the check and the drain: start again next tick.
        player.generation = generation;
        player.plan = None;
        player.resampler.clear();
        out.fill(0);
        return;
    }
    if player.plan.is_none() {
        if player.resampler.held() < LINE_IN_START_CHUNKS * frames {
            out.fill(0);
            return;
        }
        match LatencyPlan::new(*growth, 0, LOCAL_LATENCY_NS) {
            Ok(mut plan) => {
                let _ = plan.set_target(player.target_ns);
                player.plan = Some(plan);
                counts.starts += 1;
            }
            Err(_) => {
                out.fill(0);
                return;
            }
        }
    }
    let Some(plan) = player.plan.as_mut() else {
        out.fill(0);
        return;
    };
    let mut trial = plan.clone();
    let chunk = trial.next_chunk();
    if player.resampler.render(&chunk, &mut player.out).is_err() {
        counts.underruns += 1;
        port.count_underrun();
        out.fill(0);
        return;
    }
    *plan = trial;
    // The plan's first chunk is LOOKAHEAD_FRAMES short: hold its first frame.
    let short = frames.saturating_sub(chunk.frames as usize);
    let mut at = 0usize;
    for _ in 0..short {
        for c in 0..channels {
            encode_sample(player.out[c], format, &mut out[at..at + width]);
            at += width;
        }
    }
    for x in player.out.iter().take((frames - short) * channels) {
        encode_sample(*x, format, &mut out[at..at + width]);
        at += width;
    }
    out[at..].fill(0);
}

/// Fill `buf` from `source`: `Ok(true)` when it is full, `Ok(false)` when the
/// source ended first (the rest of `buf` is then zeros).
fn fill(source: &mut dyn PcmSource, buf: &mut [u8]) -> io::Result<bool> {
    let mut at = 0usize;
    while at < buf.len() {
        match source.read(&mut buf[at..]) {
            Ok(0) => {
                buf[at..].fill(0);
                return Ok(false);
            }
            Ok(n) => at += n,
            Err(ref e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        }
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    use chorus_audio::StreamFormat;
    use chorus_protocol::{decode_frame, FrameOutcome};

    use crate::router::SessionStart;
    use crate::stream::SUBSCRIBER_QUEUE_LIMIT;

    struct Constant(u8);

    impl PcmSource for Constant {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            buf.fill(self.0);
            Ok(buf.len())
        }
        fn describe(&self) -> String {
            "constant".to_string()
        }
    }

    fn chunk_of(item: &Outbound) -> AudioChunk {
        match item {
            Outbound::Frame(bytes) => match decode_frame(bytes).outcome {
                FrameOutcome::Decoded(Message::AudioChunk(c)) => c,
                other => panic!("not a chunk: {:?}", other),
            },
            other => panic!("not a frame: {:?}", other),
        }
    }

    #[test]
    fn a_chime_repeats_with_its_gap_across_chunk_boundaries() {
        let chime = [1u8, 2, 3, 4, 5];
        let mut out = [9u8; 4];
        let mut at = 0;
        let mut heard = Vec::new();
        for _ in 0..4 {
            at = play_chime(&chime, 3, at, &mut out);
            heard.extend_from_slice(&out);
        }
        assert_eq!(heard, [1, 2, 3, 4, 5, 0, 0, 0, 1, 2, 3, 4, 5, 0, 0, 0]);
    }

    #[test]
    fn a_line_in_waits_for_its_start_fill_then_plays_its_source_in_order() {
        let frames = 960usize;
        let port = Port::new(48_000, 2);
        let growth = GrowthConfig {
            sample_rate_hz: 48_000,
            chunk_frames: frames as u32,
            max_rate_deviation: MAX_RATE_DEVIATION,
            ramp_frames: RAMP_MS * 48,
        };
        let mut player = LineInPlayer {
            generation: u64::MAX,
            plan: None,
            resampler: CubicResampler::with_capacity(2, 96_000),
            out: Vec::with_capacity(frames * 2),
            target_ns: LOCAL_LATENCY_NS,
        };
        let mut counts = LineInCounts::default();
        let mut out = vec![0u8; frames * 4];
        let chunk = |k: usize| -> Vec<u8> {
            (k * frames..(k + 1) * frames)
                .flat_map(|i| {
                    let b = (1_000 + i as i16).to_le_bytes();
                    [b[0], b[1], b[0], b[1]]
                })
                .collect()
        };
        let fmt = SampleFormat::PcmS16Le;
        port.reset();
        assert!(port.write_pcm(&chunk(0), fmt));
        play_line_in(
            &mut player,
            &port,
            &growth,
            frames,
            fmt,
            &mut out,
            &mut counts,
        );
        assert!(
            out.iter().all(|b| *b == 0),
            "one chunk is not the start fill"
        );
        assert!(port.write_pcm(&chunk(1), fmt));
        let mut heard: Vec<i16> = Vec::new();
        for k in 2..6 {
            play_line_in(
                &mut player,
                &port,
                &growth,
                frames,
                fmt,
                &mut out,
                &mut counts,
            );
            heard.extend(
                out.as_chunks::<4>()
                    .0
                    .iter()
                    .map(|f| i16::from_le_bytes([f[0], f[1]])),
            );
            assert!(port.write_pcm(&chunk(k), fmt));
        }
        assert_eq!(counts.starts, 1);
        assert_eq!(counts.underruns, 0);
        // The first frame is held for the plan's two-frame lookahead, then
        // every source frame in order: a straight line comes back exact.
        assert_eq!(&heard[..3], &[1_000, 1_000, 1_000]);
        assert!(
            heard[2..].windows(2).all(|w| w[1] == w[0] + 1),
            "{:?}",
            &heard[..8]
        );
        // A tick with too little upstream plays silence and the plan waits:
        // source chunks 4 and 5 play, the next has not arrived.
        for _ in 0..3 {
            play_line_in(
                &mut player,
                &port,
                &growth,
                frames,
                fmt,
                &mut out,
                &mut counts,
            );
        }
        assert_eq!(counts.underruns, 1);
        assert!(out.iter().all(|b| *b == 0));
    }

    #[test]
    fn every_slot_is_cut_on_one_grid_and_plays_its_own_input() {
        let router = Router::slotted(2);
        let mut inboxes = Vec::new();
        for route in 0..3 {
            let (out, inbox) = mpsc::sync_channel(SUBSCRIBER_QUEUE_LIMIT);
            router.register(
                "e",
                0,
                out,
                &SessionStart {
                    route,
                    ..SessionStart::default()
                },
            );
            inboxes.push(inbox);
        }
        let (commands_tx, commands) = mpsc::sync_channel(8);
        commands_tx
            .send(SlotCommand::Input {
                slot: 0,
                input: SlotInput::Stream,
            })
            .unwrap();
        let (streams_tx, streams) = mpsc::channel::<Box<dyn PcmSource>>();
        streams_tx.send(Box::new(Constant(7))).unwrap();
        let (events, _events) = mpsc::channel();
        let keep = Arc::new(AtomicBool::new(true));
        let params = ServeParams {
            format: StreamFormat::new(48_000, 2, "pcm_s16le").unwrap(),
            chunk_us: 5_000,
            rate_skew_ppm: 0,
        };
        let stopper = {
            let keep = Arc::clone(&keep);
            thread::spawn(move || {
                thread::sleep(Duration::from_millis(60));
                keep.store(false, Ordering::SeqCst);
            })
        };
        let report = serve_slots(
            params,
            MonotonicTimeline::new(),
            &router,
            &SlotMedia::default(),
            &commands,
            &streams,
            &events,
            &keep,
        )
        .unwrap();
        stopper.join().unwrap();
        assert!(report.ticks >= 3, "{:?}", report);
        let got: Vec<Vec<AudioChunk>> = inboxes
            .iter()
            .map(|i| i.try_iter().map(|o| chunk_of(&o)).collect())
            .collect();
        for slot in &got {
            assert_eq!(
                slot.len() as u64,
                report.ticks,
                "one chunk per tick on every fanout"
            );
        }
        for (k, ((a, b), idle)) in got[0].iter().zip(&got[1]).zip(&got[2]).enumerate() {
            assert_eq!(a.sequence, k as u32);
            assert_eq!(b.sequence, a.sequence);
            assert_eq!(idle.timestamp_ns, a.timestamp_ns, "one grid");
            assert!(
                a.audio_data.iter().all(|x| *x == 7),
                "slot 0 plays the stream"
            );
            assert!(b.audio_data.iter().all(|x| *x == 0), "slot 1 plays silence");
            assert!(
                idle.audio_data.iter().all(|x| *x == 0),
                "and so does the idle one"
            );
        }
        for w in got[0].windows(2) {
            assert_eq!(w[1].timestamp_ns - w[0].timestamp_ns, 5_000_000);
        }
    }
}
