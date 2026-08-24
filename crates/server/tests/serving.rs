//! The server, on a real socket, decoded by the client's real receiver.
//!
//! These run anywhere: no audio device, no privilege, no container. What they
//! assert is everything about the bytes between the two processes, which is
//! the part that has to be right before a device is worth pointing at.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use chorus_audio::{MonotonicTimeline, StreamFormat};
use chorus_client_linux::receive::{FramingError, Received, Receiver};
use chorus_protocol::{decode_frame, encode, AudioChunk, FrameOutcome, Message, MessageType};
use chorus_server::serve::{serve_stream, ServeParams, ServeReport};

const CHUNK_US: u64 = 20_000;
const CHUNK_NS: u64 = CHUNK_US * 1_000;

fn format() -> StreamFormat {
    StreamFormat::new(48_000, 2, "pcm_s16le").unwrap()
}

fn params(skew_ppm: u64) -> ServeParams {
    ServeParams {
        format: format(),
        chunk_us: CHUNK_US,
        rate_skew_ppm: skew_ppm,
    }
}

/// A ramp that is recognisable at every offset, so a dropped, repeated or
/// reordered byte shows up in the concatenation.
fn ramp(len: usize) -> Vec<u8> {
    (0..len).map(|i| (i % 251) as u8).collect()
}

/// What one served run did, and every byte the client received from it.
struct Served {
    report: ServeReport,
    wire: Vec<u8>,
}

/// A timeline whose origin is emphatically not zero.
///
/// The server stamps its first chunk with `timeline.now_ns()`, so a timeline
/// created and immediately used has an origin of a few microseconds and a bug
/// that ignores the origin is nearly invisible. Starting the timeline a
/// measurable interval before the run makes the origin a number no arithmetic
/// over the chunk count can produce by accident, which is the difference
/// between "the stream lasted this long" and "the stream ends here".
fn aged_timeline(at_least_ns: u64) -> MonotonicTimeline {
    let timeline = MonotonicTimeline::new();
    while timeline.now_ns() < at_least_ns {
        thread::sleep(Duration::from_millis(1));
    }
    timeline
}

/// Serve `input` to one client over a real TCP connection on `timeline`, and
/// give back the report and every byte the client received.
fn serve_over_tcp_on(
    params: ServeParams,
    timeline: MonotonicTimeline,
    input: Vec<u8>,
    should_continue: impl Fn() -> bool + Send + 'static,
) -> Served {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let addr = listener.local_addr().unwrap();

    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("the client connects");
        let mut at = 0usize;
        let mut read = move |buf: &mut [u8]| -> std::io::Result<usize> {
            let n = buf.len().min(input.len() - at);
            buf[..n].copy_from_slice(&input[at..at + n]);
            at += n;
            Ok(n)
        };
        serve_stream(params, timeline, &mut read, &mut stream, &should_continue)
            .expect("the stream serves")
    });

    let mut client = TcpStream::connect(addr).expect("the server is listening");
    let mut received = Vec::new();
    client.read_to_end(&mut received).expect("the stream ends");
    let report = server.join().expect("the server thread finishes");
    Served {
        report,
        wire: received,
    }
}

/// Serve `input` to one client over a real TCP connection and give back every
/// byte the client received.
fn serve_over_tcp(params: ServeParams, input: Vec<u8>) -> Vec<u8> {
    serve_over_tcp_on(params, MonotonicTimeline::new(), input, || true).wire
}

/// The one `stream_end` on the wire, and the last `audio_chunk` before it.
///
/// Both come from decoding the same connection, which is the point: the
/// end-of-stream relation is graded against the chunk the client actually
/// received, never against a constant recomputed from the server's own
/// arithmetic.
fn end_and_final_chunk(messages: &[Message]) -> (chorus_protocol::StreamEnd, AudioChunk) {
    let end = match messages.last().expect("the run produced messages") {
        Message::StreamEnd(end) => *end,
        other => panic!("the last message is {:?}, not the in-band end of stream", other),
    };
    let final_chunk = chunks_of(messages)
        .last()
        .copied()
        .expect("a clean end follows at least one chunk")
        .clone();
    (end, final_chunk)
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

fn chunks_of(messages: &[Message]) -> Vec<&AudioChunk> {
    messages
        .iter()
        .filter_map(|m| match m {
            Message::AudioChunk(c) => Some(c),
            _ => None,
        })
        .collect()
}

#[test]
fn the_chunks_a_client_receives_concatenate_back_to_the_input_exactly() {
    let format = format();
    let frames_per_chunk = format.frames_in(CHUNK_US).unwrap();
    // Ten whole chunks, then 100 whole frames, then three bytes that are not a
    // whole frame.
    let body = frames_per_chunk * 10 * format.frame_len() + 100 * format.frame_len();
    let input = ramp(body + 3);

    let wire = serve_over_tcp(params(0), input.clone());
    let messages = decode_all(&wire);
    let chunks = chunks_of(&messages);

    assert_eq!(chunks.len(), 11);
    let mut rebuilt = Vec::new();
    for (i, c) in chunks.iter().enumerate() {
        let frames = c.audio_data.len() / format.frame_len();
        if i < 10 {
            assert_eq!(frames, frames_per_chunk, "chunk {} is not a full chunk", i);
        } else {
            assert_eq!(frames, 100, "only the final chunk is short, and it is not empty");
        }
        rebuilt.extend_from_slice(&c.audio_data);
    }
    assert_eq!(
        rebuilt,
        input[..body],
        "the chunks concatenated are the input, with nothing inserted, dropped or repeated"
    );
}

#[test]
fn sequence_numbers_are_a_contiguous_run_and_timestamps_are_evenly_spaced() {
    let format = format();
    let frames_per_chunk = format.frames_in(CHUNK_US).unwrap();
    let input = ramp(frames_per_chunk * 25 * format.frame_len() + 400 * format.frame_len());

    let wire = serve_over_tcp(params(0), input);
    let messages = decode_all(&wire);
    let chunks = chunks_of(&messages);
    assert_eq!(chunks.len(), 26);

    for (i, c) in chunks.iter().enumerate() {
        assert_eq!(c.sequence, i as u32, "the sequence column is contiguous");
    }
    for pair in chunks.windows(2) {
        assert!(
            pair[1].timestamp_ns > pair[0].timestamp_ns,
            "timestamps are strictly increasing"
        );
        assert_eq!(
            pair[1].timestamp_ns - pair[0].timestamp_ns,
            CHUNK_US * 1_000,
            "the delta is the configured chunk duration, start to start, including across the \
             short final chunk"
        );
    }
}

#[test]
fn the_bytes_on_the_wire_decode_with_the_committed_protocol_unchanged() {
    let format = format();
    let input = ramp(format.frames_in(CHUNK_US).unwrap() * 3 * format.frame_len());
    let wire = serve_over_tcp(params(0), input);

    // Every frame on the wire is a catalogued type, decodes, and the run ends
    // with the in-band stream end.
    let mut at = 0usize;
    let mut kinds = Vec::new();
    while at < wire.len() {
        let r = decode_frame(&wire[at..]);
        assert!(r.consumed > 0, "every frame on the wire is complete");
        match r.outcome {
            FrameOutcome::Decoded(m) => kinds.push(m.message_type()),
            other => panic!("a frame did not decode: {:?}", other),
        }
        at += r.consumed;
    }
    assert_eq!(kinds.len(), 4);
    assert!(kinds[..3]
        .iter()
        .all(|k| *k == MessageType::AudioChunk));
    assert_eq!(kinds[3], MessageType::StreamEnd);
}

#[test]
fn a_clean_end_sends_the_signal_in_band_and_a_stopped_run_does_not() {
    let format = format();
    let input = ramp(format.frames_in(CHUNK_US).unwrap() * 2 * format.frame_len());
    // Two milliseconds of timeline before the run, so the origin the chunker is
    // built on is not zero and a value that ignored it would be visible.
    let served = serve_over_tcp_on(params(0), aged_timeline(2_000_000), input, || true);
    let messages = decode_all(&served.wire);
    let (end, final_chunk) = end_and_final_chunk(&messages);

    assert!(
        final_chunk.timestamp_ns >= 2_000_000,
        "this run was served on a timeline whose origin is not zero, and the first chunk carries \
         it; the final chunk is stamped {} ns",
        final_chunk.timestamp_ns
    );
    assert_eq!(end.final_sequence, 1);
    assert_eq!(
        end.end_timestamp_ns,
        final_chunk.timestamp_ns + CHUNK_NS,
        "docs/protocol.md: end_timestamp_ns is one configured chunk duration past the \
         presentation timestamp of the final chunk decoded off this same connection"
    );
    assert_ne!(
        end.end_timestamp_ns,
        served.report.chunks_sent * CHUNK_NS,
        "the elapsed duration of the stream is not an instant on the server timeline, and the \
         two are only equal when the origin is zero"
    );

    // The signal is data on the connection, before the close: the client sees
    // it through the ordinary receiver, not by noticing the socket shut.
    let mut receiver = Receiver::new();
    let events = receiver.push(&served.wire).expect("the stream is well framed");
    assert!(matches!(events.last(), Some(Received::End(_))));
}

#[test]
fn a_short_final_chunk_still_ends_one_configured_chunk_past_its_timestamp() {
    let format = format();
    let frames_per_chunk = format.frames_in(CHUNK_US).unwrap();
    // Three whole chunks, then 100 whole frames: a final chunk that is a
    // whole-frame remainder smaller than one chunk.
    let input = ramp((frames_per_chunk * 3 + 100) * format.frame_len());
    let served = serve_over_tcp_on(params(0), aged_timeline(2_000_000), input, || true);
    let messages = decode_all(&served.wire);
    let (end, final_chunk) = end_and_final_chunk(&messages);

    let final_frames = final_chunk.audio_data.len() / format.frame_len();
    assert_eq!(final_frames, 100, "the final chunk of this run is short");
    assert!(final_frames < frames_per_chunk);
    assert_eq!(end.final_sequence, 3);
    assert_eq!(
        end.end_timestamp_ns,
        final_chunk.timestamp_ns + CHUNK_NS,
        "the relation uses the CONFIGURED chunk duration, so a short final chunk does not shorten \
         it"
    );

    // The short chunk's own duration would be a different number, and it is the
    // one this criterion exists to refuse.
    let short_chunk_ns = (final_frames as u64) * 1_000_000_000 / 48_000;
    assert_ne!(
        end.end_timestamp_ns,
        final_chunk.timestamp_ns + short_chunk_ns,
        "the final chunk's own duration is not the duration the relation adds"
    );
}

#[test]
fn a_run_that_does_not_end_cleanly_emits_no_end_of_stream_at_all() {
    let format = format();
    let input = ramp(format.frames_in(CHUNK_US).unwrap() * 4 * format.frame_len());
    let served = serve_over_tcp_on(params(0), aged_timeline(2_000_000), input, || false);

    assert!(!served.report.ended_cleanly);
    assert_eq!(served.report.chunks_sent, 0);
    assert!(
        served.wire.is_empty(),
        "a stopped run puts nothing on the wire, least of all an end-of-stream signal"
    );
    assert!(
        decode_all(&served.wire).is_empty(),
        "and there is no stream_end to decode"
    );
}

#[test]
fn a_run_stopped_with_chunks_already_on_the_wire_emits_no_end_of_stream_either() {
    let format = format();
    // Twenty chunks of input, so the run has plenty left when it is stopped.
    let input = ramp(format.frames_in(CHUNK_US).unwrap() * 20 * format.frame_len());
    // True for the first three passes of the serve loop and false afterwards.
    // That is the half of this criterion the zero-chunk case cannot reach: here
    // there IS a final chunk to name and a timestamp to end one chunk past, so
    // the only thing suppressing the signal is that the run was stopped rather
    // than the source ending. A check that only ever stopped a run before its
    // first chunk would stay green if that suppression were deleted.
    let passes = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&passes);
    let served = serve_over_tcp_on(params(0), aged_timeline(2_000_000), input, move || {
        counter.fetch_add(1, Ordering::SeqCst) < 3
    });

    assert!(
        served.report.chunks_sent > 0,
        "this case is only the interesting one if chunks reached the wire before the stop"
    );
    assert!(!served.report.ended_cleanly);
    let messages = decode_all(&served.wire);
    assert!(
        !chunks_of(&messages).is_empty(),
        "the client received chunks, so the server had a final chunk to name"
    );
    assert!(
        !messages
            .iter()
            .any(|m| m.message_type() == MessageType::StreamEnd),
        "a stopped run sends no end-of-stream signal even with a final chunk behind it: {:?}",
        messages.iter().map(|m| m.message_type()).collect::<Vec<_>>()
    );
    assert!(
        matches!(messages.last(), Some(Message::AudioChunk(_))),
        "the last thing on the wire is a chunk, not an end of stream"
    );
}

#[test]
fn a_clean_end_with_no_chunk_sent_emits_no_end_of_stream_at_all() {
    let format = format();
    // Three bytes is less than one four-byte frame, so the source ends cleanly
    // having produced no chunk at all.
    assert_eq!(format.frame_len(), 4);
    let served = serve_over_tcp_on(params(0), aged_timeline(2_000_000), ramp(3), || true);

    assert_eq!(served.report.chunks_sent, 0);
    assert_eq!(served.report.bytes_discarded, 3);
    assert!(
        !served.report.ended_cleanly,
        "there is no final sequence to name and no timestamp to end one chunk past"
    );
    let messages = decode_all(&served.wire);
    assert!(
        !messages
            .iter()
            .any(|m| m.message_type() == MessageType::StreamEnd),
        "no stream_end was emitted: {:?}",
        messages
    );
    assert!(served.wire.is_empty());
}

#[test]
fn a_skewed_run_emits_faster_without_moving_a_single_timestamp() {
    let format = format();
    let frames_per_chunk = format.frames_in(CHUNK_US).unwrap();
    let input = ramp(frames_per_chunk * 6 * format.frame_len());

    let plain = decode_all(&serve_over_tcp(params(0), input.clone()));
    let skewed = decode_all(&serve_over_tcp(params(2_000), input));

    // Each run has its own monotonic epoch, so the comparable quantity is the
    // spacing rather than the absolute value. That is the whole point: the
    // spacing is a property of the stream and the epoch is not.
    let deltas = |messages: &[Message]| -> Vec<u64> {
        chunks_of(messages)
            .windows(2)
            .map(|w| w[1].timestamp_ns - w[0].timestamp_ns)
            .collect()
    };
    let plain_deltas = deltas(&plain);
    assert_eq!(plain_deltas, vec![CHUNK_US * 1_000; 5]);
    assert_eq!(
        plain_deltas,
        deltas(&skewed),
        "the deliberate rate difference is in the pacing, never in the timeline"
    );
}

#[test]
fn a_truncated_chunk_injected_on_the_transport_is_caught_rather_than_played() {
    // The injection the failure-path criterion names, on the transport
    // actually chosen: a frame header declaring a full chunk, with fewer bytes
    // really belonging to it, and the next frame's bytes filling the gap.
    let format = format();
    let frames_per_chunk = format.frames_in(CHUNK_US).unwrap();
    let chunk = |sequence: u32, frames: usize| {
        encode(&Message::AudioChunk(AudioChunk {
            sequence,
            timestamp_ns: u64::from(sequence) * CHUNK_US * 1_000,
            sample_rate_hz: 48_000,
            channels: 2,
            sample_format: chorus_protocol::SampleFormat::PcmS16Le,
            reserved: [0u8; chorus_protocol::RESERVED_LEN],
            audio_data: vec![7u8; frames * format.frame_len()],
        }))
        .unwrap()
    };

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let full = chunk(0, frames_per_chunk);
    let next = chunk(1, frames_per_chunk);
    let third = chunk(2, frames_per_chunk);
    thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream.write_all(&full).unwrap();
        // Declare a full chunk and deliver half of it.
        let short = 3 + 32 + (frames_per_chunk / 2) * 4;
        stream.write_all(&next[..short]).unwrap();
        stream.write_all(&third).unwrap();
        stream.flush().unwrap();
    });

    let mut client = TcpStream::connect(addr).unwrap();
    let mut bytes = Vec::new();
    let _ = client.read_to_end(&mut bytes);

    let mut receiver = Receiver::new();
    let mut frames_accepted = 0u64;
    let mut framing_error: Option<FramingError> = None;
    match receiver.push(&bytes) {
        Ok(events) => {
            for e in events {
                if let Received::Chunk { frames, .. } = e {
                    frames_accepted += frames;
                }
            }
        }
        Err(e) => framing_error = Some(e),
    }

    let error = framing_error.expect("the client reports a framing error rather than playing on");
    assert!(
        error.to_string().contains("framing error"),
        "the error is typed and says what it is: {}",
        error
    );
    assert!(
        frames_accepted <= 2 * frames_per_chunk as u64,
        "no mis-framed bytes turned into extra audio"
    );
}

#[test]
fn a_duplicate_sequence_on_the_transport_is_discarded_and_the_session_stays_open() {
    let format = format();
    let frames_per_chunk = format.frames_in(CHUNK_US).unwrap();
    let mut wire = Vec::new();
    for i in 0..4u32 {
        wire.extend_from_slice(
            &encode(&Message::AudioChunk(AudioChunk {
                sequence: i,
                timestamp_ns: u64::from(i) * CHUNK_US * 1_000,
                sample_rate_hz: 48_000,
                channels: 2,
                sample_format: chorus_protocol::SampleFormat::PcmS16Le,
                reserved: [0u8; chorus_protocol::RESERVED_LEN],
                audio_data: vec![1u8; frames_per_chunk * format.frame_len()],
            }))
            .unwrap(),
        );
    }
    let mut receiver = Receiver::new();
    let events = receiver.push(&wire).expect("well framed");
    assert_eq!(events.len(), 4);
    // The buffer, not the receiver, is what decides a duplicate; the receiver's
    // job here is only to keep the session open, which it did.
    assert!(events.iter().all(|e| matches!(e, Received::Chunk { .. })));
}
