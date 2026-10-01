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
//! drained with `try_recv` at the top of each tick, so the conductor that
//! decides it never blocks this thread and this thread never waits for it.
//! This change carries two inputs, [`SlotInput::Silence`] and
//! [`SlotInput::Stream`] (the configured `--source`, read ONCE per tick
//! however many slots play it); the enum and the channel are where the
//! generated chimes and the endpoints' line-ins join (the next goal 11 track).
//!
//! The configured stream is handed to this thread over a channel, as the
//! one-stream shape hands it, and when it ends (a file's last byte) the
//! slots playing it play silence until the supervisor hands over the next
//! one. There is no `stream_end` in this shape: a slot does not end, its
//! input changes.
//!
//! No clock but the monotonic timeline is read here, and nothing here waits
//! on anything but the pace of the grid.

use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, TryRecvError};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use chorus_audio::MonotonicTimeline;
use chorus_protocol::{encode, AudioChunk, Message, RESERVED_LEN};

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
}

impl SlotInput {
    /// The word a status line uses.
    pub fn name(self) -> &'static str {
        match self {
            SlotInput::Silence => "silence",
            SlotInput::Stream => "stream",
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
}

/// Serve every slot until `keep` says stop or the configured stream fails.
///
/// `router` holds the fanouts (slot by slot, then the silent one) and the grid
/// guard; `commands` are drained once per tick; `streams` delivers the
/// configured stream, and again after each [`SlotEvent::StreamEnded`].
pub fn serve_slots(
    params: ServeParams,
    timeline: MonotonicTimeline,
    router: &Router,
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
    let bytes_per_chunk = frames * params.format.frame_len();
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
        // At the chunk boundary: what each slot plays from this tick on.
        loop {
            match commands.try_recv() {
                Ok(SlotCommand::Input { slot, input }) => {
                    if let Some(at) = inputs.get_mut(slot) {
                        *at = input;
                    }
                }
                Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => break,
            }
        }
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
        // plays it: a file nobody is listening to waits where it is.
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
            let _one_tick = router.grid();
            for (slot, input) in inputs.iter().enumerate() {
                let frame = match (input, &playing) {
                    (SlotInput::Stream, Some(frame)) => frame.clone(),
                    _ => quiet.clone(),
                };
                router.fanouts()[slot].broadcast(frame);
            }
            router.fanouts()[router.idle()].broadcast(quiet);
        }
        report.ticks += 1;
        report.final_sequence = sequence;
        sequence = sequence.wrapping_add(1);
    }
    Ok(report)
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
        for k in 0..got[0].len() {
            assert_eq!(got[0][k].sequence, k as u32);
            assert_eq!(got[1][k].sequence, got[0][k].sequence);
            assert_eq!(got[2][k].timestamp_ns, got[0][k].timestamp_ns, "one grid");
            assert!(
                got[0][k].audio_data.iter().all(|b| *b == 7),
                "slot 0 plays the stream"
            );
            assert!(
                got[1][k].audio_data.iter().all(|b| *b == 0),
                "slot 1 plays silence"
            );
            assert!(
                got[2][k].audio_data.iter().all(|b| *b == 0),
                "and so does the idle one"
            );
        }
        for w in got[0].windows(2) {
            assert_eq!(w[1].timestamp_ns - w[0].timestamp_ns, 5_000_000);
        }
    }
}
