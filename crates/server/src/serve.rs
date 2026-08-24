//! Ingest, chunk, stamp, pace, send.
//!
//! # The timeline
//!
//! One monotonic timeline, and presentation timestamps that advance by exactly
//! the configured chunk duration, start to start. That spacing is a property
//! of the stream, not of when a chunk happened to be sent, so it does not move
//! when the machine is busy and it does not move when a run is deliberately
//! skewed.
//!
//! # Pacing, and why it is separate from the timeline
//!
//! Chunks are emitted at wall-clock-free intervals derived from the same
//! monotonic timeline. `rate_skew_ppm` makes those intervals shorter, which
//! makes a client's buffer climb; it does **not** touch the timestamps. That
//! separation is what makes the overflow verification a test of the client's
//! ceiling behaviour rather than a test of a broken timeline.
//!
//! # The end of a stream
//!
//! A source that delivered its last byte and closed without error ended
//! cleanly. The final chunk is a whole number of frames, greater than zero, no
//! more than the configured duration, and is followed by a `stream_end`
//! message in band, on the connection, before the close. A source that failed
//! mid-read gets no such signal, and the client will report the difference.
//!
//! `stream_end` carries `end_timestamp_ns`, and `docs/protocol.md` is the
//! normative definition of it: the presentation timestamp of the final chunk
//! plus one configured chunk duration, in nanoseconds on the server timeline.
//! So it is derived here from the timestamp actually put on the final chunk,
//! which carries this stream's timeline origin, and never from the number of
//! chunks sent - that product is an elapsed duration and it is only an instant
//! on the timeline when the origin happens to be zero, which outside a unit
//! test it never is.

use std::io::{self, Write};
use std::thread;
use std::time::Duration;

use chorus_audio::{Chunk, Chunker, MonotonicTimeline, StreamFormat};
use chorus_protocol::{encode, Message, StreamEnd};

/// What one served stream did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ServeReport {
    /// Chunks put on the wire.
    pub chunks_sent: u64,
    /// Frames put on the wire.
    pub frames_sent: u64,
    /// Bytes of a trailing partial frame that were discarded.
    pub bytes_discarded: u64,
    /// Whether the stream ended cleanly and the in-band signal was sent.
    pub ended_cleanly: bool,
    /// Sequence of the final chunk, when there was one.
    pub final_sequence: u32,
}

/// Why serving stopped.
#[derive(Debug)]
pub enum ServeError {
    /// Reading the source failed.
    Source(io::Error),
    /// Writing to the client failed.
    Transport(io::Error),
    /// A chunk could not be encoded, which would be a bug in the chunker.
    Encode(String),
}

impl std::fmt::Display for ServeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ServeError::Source(e) => write!(f, "the PCM source failed: {}", e),
            ServeError::Transport(e) => write!(f, "the connection to the client failed: {}", e),
            ServeError::Encode(e) => write!(f, "a chunk could not be encoded: {}", e),
        }
    }
}

impl std::error::Error for ServeError {}

/// How a served stream is shaped.
#[derive(Debug, Clone, Copy)]
pub struct ServeParams {
    /// The stream format.
    pub format: StreamFormat,
    /// Chunk duration, microseconds.
    pub chunk_us: u64,
    /// How much faster than real time to emit, in parts per million.
    pub rate_skew_ppm: u64,
}

impl ServeParams {
    /// The interval between chunk emissions, in nanoseconds.
    ///
    /// The chunk duration divided by `1 + skew`, computed in integers so that
    /// a run is reproducible.
    pub fn emit_interval_ns(&self) -> u64 {
        let nominal = self.chunk_us * 1_000;
        nominal * 1_000_000 / (1_000_000 + self.rate_skew_ppm)
    }
}

/// Serve one stream to one client.
///
/// `read_source` is called for more PCM; `Ok(0)` means the source ended
/// cleanly. `sink` is the connection.
pub fn serve_stream<S, W, F>(
    params: ServeParams,
    timeline: MonotonicTimeline,
    read_source: &mut F,
    sink: &mut W,
    should_continue: &S,
) -> Result<ServeReport, ServeError>
where
    S: Fn() -> bool,
    W: Write,
    F: FnMut(&mut [u8]) -> io::Result<usize>,
{
    let mut chunker = Chunker::new(params.format, params.chunk_us, 0, timeline.now_ns())
        .map_err(|e| ServeError::Encode(e.to_string()))?;
    let mut report = ServeReport::default();
    let mut scratch = vec![0u8; chunker.bytes_per_chunk().max(4096)];
    let interval_ns = params.emit_interval_ns();
    let mut next_emit_ns = timeline.now_ns();
    // The sequence and the presentation timestamp of the last chunk actually
    // put on the wire. Both come off the chunk itself rather than being
    // recomputed, because the end-of-stream message has to describe the bytes
    // the client received.
    let mut final_chunk: Option<(u32, u64)> = None;
    let mut source_ended = false;

    loop {
        if !should_continue() {
            // Deliberately no end-of-stream signal: the run was stopped, the
            // source did not end.
            return Ok(report);
        }

        let n = if source_ended {
            0
        } else {
            match read_source(&mut scratch) {
                Ok(n) => n,
                Err(e) => return Err(ServeError::Source(e)),
            }
        };

        let chunks: Vec<Chunk> = if n == 0 {
            source_ended = true;
            let (final_chunk, chunker_report) = chunker.finish();
            report.bytes_discarded = chunker_report.bytes_discarded;
            final_chunk.into_iter().collect()
        } else {
            chunker.push(&scratch[..n])
        };

        for chunk in chunks {
            // Pace against the monotonic timeline, never against a sleep that
            // accumulates its own error.
            let now = timeline.now_ns();
            if next_emit_ns > now {
                let wait = next_emit_ns - now;
                if wait > 1_000 {
                    thread::sleep(Duration::from_nanos(wait));
                }
            }
            next_emit_ns = next_emit_ns.saturating_add(interval_ns);

            let frame = encode(&Message::AudioChunk(chunk.message.clone()))
                .map_err(|e| ServeError::Encode(e.to_string()))?;
            sink.write_all(&frame).map_err(ServeError::Transport)?;
            report.chunks_sent += 1;
            report.frames_sent += chunk.frames as u64;
            final_chunk = Some((chunk.message.sequence, chunk.message.timestamp_ns));
        }

        if source_ended {
            break;
        }
    }

    // The source ended cleanly, so the end of stream is announced in band,
    // after the final chunk and before the close. A client that sees this
    // knows the stream is over; one that does not knows it lost the server.
    // No chunk on the wire means there is nothing for a final sequence to name
    // and no presentation timestamp to end one chunk past, so no signal is
    // sent. A run stopped by `should_continue` returned above, for the other
    // reason: the source did not end.
    if let Some((sequence, final_timestamp_ns)) = final_chunk {
        let end = StreamEnd {
            final_sequence: sequence,
            // The relation `docs/protocol.md` defines, and the CONFIGURED
            // chunk duration rather than the final chunk's own: only the last
            // chunk of a stream may be short, and when it is, this instant is
            // a little past the point the audio stops.
            end_timestamp_ns: final_timestamp_ns.saturating_add(chunker.chunk_ns()),
        };
        let frame = encode(&Message::StreamEnd(end))
            .map_err(|e| ServeError::Encode(e.to_string()))?;
        sink.write_all(&frame).map_err(ServeError::Transport)?;
        sink.flush().map_err(ServeError::Transport)?;
        report.ended_cleanly = true;
        report.final_sequence = sequence;
    }

    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chorus_protocol::{decode_frame, FrameOutcome};

    fn params(chunk_us: u64, skew: u64) -> ServeParams {
        ServeParams {
            format: StreamFormat::new(48_000, 2, "pcm_s16le").unwrap(),
            chunk_us,
            rate_skew_ppm: skew,
        }
    }

    fn decode_all(bytes: &[u8]) -> Vec<Message> {
        let mut out = Vec::new();
        let mut at = 0;
        while at < bytes.len() {
            let r = decode_frame(&bytes[at..]);
            if r.consumed == 0 {
                break;
            }
            at += r.consumed;
            if let FrameOutcome::Decoded(m) = r.outcome {
                out.push(m);
            }
        }
        out
    }

    #[test]
    fn a_clean_end_is_announced_in_band_after_the_final_chunk() {
        let p = params(20_000, 0);
        // Ten chunks plus 100 frames plus three bytes.
        let total = p.format.frames_in(20_000).unwrap() * 10 * p.format.frame_len()
            + 100 * p.format.frame_len()
            + 3;
        let mut left = total;
        let mut fed = 0usize;
        let mut read = move |buf: &mut [u8]| -> io::Result<usize> {
            let n = buf.len().min(left);
            for (i, b) in buf[..n].iter_mut().enumerate() {
                *b = ((fed + i) % 251) as u8;
            }
            fed += n;
            left -= n;
            Ok(n)
        };
        let mut wire = Vec::new();
        let report = serve_stream(
            p,
            MonotonicTimeline::new(),
            &mut read,
            &mut wire,
            &|| true,
        )
        .unwrap();

        assert_eq!(report.chunks_sent, 11);
        assert_eq!(report.bytes_discarded, 3);
        assert!(report.ended_cleanly);

        let messages = decode_all(&wire);
        assert_eq!(messages.len(), 12);
        // The expectation comes off the wire: the final chunk this run decoded,
        // plus one configured chunk duration. Recomputing it from the emitter's
        // own arithmetic would assert that the server agrees with itself.
        let final_stamp = match &messages[10] {
            Message::AudioChunk(c) => c.timestamp_ns,
            other => panic!("message 10 is {:?}, not the final chunk", other),
        };
        match messages.last().unwrap() {
            Message::StreamEnd(end) => {
                assert_eq!(end.final_sequence, 10);
                assert_eq!(end.end_timestamp_ns, final_stamp + 20_000_000);
            }
            other => panic!("the last message is {:?}, not a stream end", other),
        }
        // Only the last chunk is short, and it is not empty.
        for (i, m) in messages[..11].iter().enumerate() {
            match m {
                Message::AudioChunk(c) => {
                    let frames = c.audio_data.len() / 4;
                    if i < 10 {
                        assert_eq!(frames, 960);
                    } else {
                        assert_eq!(frames, 100);
                    }
                    assert_eq!(c.sequence, i as u32);
                }
                other => panic!("message {} is {:?}", i, other),
            }
        }
    }

    #[test]
    fn timestamps_are_evenly_spaced_whatever_the_skew_does_to_the_pacing() {
        for skew in [0u64, 2_000, 50_000] {
            let p = params(20_000, skew);
            let mut left = p.format.frames_in(20_000).unwrap() * 5 * p.format.frame_len();
            let mut read = move |buf: &mut [u8]| -> io::Result<usize> {
                let n = buf.len().min(left);
                for b in buf[..n].iter_mut() {
                    *b = 0;
                }
                left -= n;
                Ok(n)
            };
            let mut wire = Vec::new();
            serve_stream(p, MonotonicTimeline::new(), &mut read, &mut wire, &|| true).unwrap();
            let stamps: Vec<u64> = decode_all(&wire)
                .into_iter()
                .filter_map(|m| match m {
                    Message::AudioChunk(c) => Some(c.timestamp_ns),
                    _ => None,
                })
                .collect();
            assert_eq!(stamps.len(), 5);
            for w in stamps.windows(2) {
                assert_eq!(w[1] - w[0], 20_000_000, "skew {} moved the timeline", skew);
            }
        }
    }

    #[test]
    fn a_skewed_run_emits_faster_than_real_time_without_touching_the_timeline() {
        assert_eq!(params(20_000, 0).emit_interval_ns(), 20_000_000);
        // 2000 ppm faster is 0.2 percent shorter.
        let skewed = params(20_000, 2_000).emit_interval_ns();
        assert!(skewed < 20_000_000);
        assert_eq!(skewed, 20_000_000 * 1_000_000 / 1_002_000);
    }

    #[test]
    fn a_stopped_run_sends_no_end_of_stream_signal() {
        let p = params(20_000, 0);
        let mut read = |buf: &mut [u8]| -> io::Result<usize> {
            for b in buf.iter_mut() {
                *b = 0;
            }
            Ok(buf.len())
        };
        let mut wire = Vec::new();
        let report = serve_stream(p, MonotonicTimeline::new(), &mut read, &mut wire, &|| false)
            .unwrap();
        assert!(!report.ended_cleanly);
        assert!(wire.is_empty());
    }
}
