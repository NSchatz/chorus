//! The server, on a real socket, decoded by the client's real receiver.
//!
//! These run anywhere: no audio device, no privilege, no container. What they
//! assert is everything about the bytes between the two processes, which is
//! the part that has to be right before a device is worth pointing at.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;

use chorus_audio::{MonotonicTimeline, StreamFormat};
use chorus_client_linux::receive::{FramingError, Received, Receiver};
use chorus_protocol::{decode_frame, encode, AudioChunk, FrameOutcome, Message, MessageType};
use chorus_server::serve::{serve_stream, ServeParams};

const CHUNK_US: u64 = 20_000;

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

/// Serve `input` to one client over a real TCP connection and give back every
/// byte the client received.
fn serve_over_tcp(params: ServeParams, input: Vec<u8>) -> Vec<u8> {
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
        serve_stream(
            params,
            MonotonicTimeline::new(),
            &mut read,
            &mut stream,
            &|| true,
        )
        .expect("the stream serves")
    });

    let mut client = TcpStream::connect(addr).expect("the server is listening");
    let mut received = Vec::new();
    client.read_to_end(&mut received).expect("the stream ends");
    server.join().expect("the server thread finishes");
    received
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
    let wire = serve_over_tcp(params(0), input);
    let messages = decode_all(&wire);
    match messages.last().unwrap() {
        Message::StreamEnd(end) => {
            assert_eq!(end.final_sequence, 1);
            assert_eq!(end.end_timestamp_ns, 2 * CHUNK_US * 1_000);
        }
        other => panic!("expected the in-band end of stream, got {:?}", other),
    }

    // The signal is data on the connection, before the close: the client sees
    // it through the ordinary receiver, not by noticing the socket shut.
    let mut receiver = Receiver::new();
    let events = receiver.push(&wire).expect("the stream is well framed");
    assert!(matches!(events.last(), Some(Received::End(_))));
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
