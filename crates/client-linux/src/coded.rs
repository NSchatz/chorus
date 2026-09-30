//! `coded_chunk` into the receive path: FLAC and Opus decoded, as PCM
//! `audio_chunk` frames, in the order the stream carried them.
//!
//! The v1 receive path (`receive.rs`) reads `audio_chunk`, `stream_end` and
//! `time_sync` frames and holds a stream to rules that make mis-framing
//! visible: the shape fixed by the first chunk, and only the last chunk short.
//! A coded stream reaches it through the same path: the session's
//! [`SecureReader`](chorus_protocol::v2::session::SecureReader) shows every
//! message to a [`CodedStream`] first, which decodes each `coded_chunk` with
//! [`Decoder`] (opened from the `stream_format` in force) and yields
//! `audio_chunk` frames in its place.
//!
//! The PCM is re-cut into chunks of `frames_per_chunk` frames (fewer when a
//! chunk of that size would not fit a frame), so that every chunk but the last
//! is full whatever the codec's packets were: an Opus pre-skip shortens the
//! first packets, and a stream of variable packets would otherwise look like a
//! truncated chunk to the receive path. What is left over is sent, short, just
//! before the `stream_end` (or before a new `stream_format`). Each chunk's
//! timestamp is its first sample's: the coded chunk's timestamp names its first
//! decoded sample (docs/protocol.md, 0x13), the pre-skip moves it on by the
//! frames dropped, and a run of decoded audio is contiguous, so a chunk cut from
//! the middle of one is timed from where that run began.

use std::io;
use std::sync::{Arc, Mutex};

use chorus_protocol::v2::session::{Translation, Translator};
use chorus_protocol::v2::{encode, Codec, Message, StreamFormat};
use chorus_protocol::{AudioChunk, SampleFormat, CHUNK_HEADER_LEN, MAX_PAYLOAD_LEN, RESERVED_LEN};

use crate::decode::Decoder;
use crate::session::Announced;

/// Decodes a session's coded chunks and cuts the PCM into `audio_chunk` frames.
#[derive(Debug)]
pub struct CodedStream {
    announced: Arc<Mutex<Announced>>,
    format: Option<StreamFormat>,
    decoder: Option<Decoder>,
    /// Decoded PCM not yet sent.
    pending: Vec<u8>,
    /// Where the current run of decoded audio starts on the server timeline,
    /// and how many of its frames have been cut into chunks already.
    run_start_ns: u64,
    run_frames: u64,
    sequence: Option<u32>,
    chunk_frames: usize,
}

fn framing(detail: String) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, detail)
}

impl CodedStream {
    /// A stream that reads the format in force from `announced`, which the
    /// session's handler keeps current.
    pub fn new(announced: Arc<Mutex<Announced>>) -> CodedStream {
        CodedStream {
            announced,
            format: None,
            decoder: None,
            pending: Vec::new(),
            run_start_ns: 0,
            run_frames: 0,
            sequence: None,
            chunk_frames: 0,
        }
    }

    /// The translator to hand to `SecureReader::set_translator`.
    pub fn into_translator(mut self) -> Translator {
        Box::new(move |m| self.translate(m))
    }

    /// What to do with one message (see the module notes).
    pub fn translate(&mut self, m: &Message) -> io::Result<Translation> {
        match m {
            Message::CodedChunk(c) => {
                self.ensure_decoder()?;
                let decoder = self.decoder.as_mut().expect("opened above");
                let skipping = decoder.skip_left().min(c.frames as usize) as u64;
                let rate = u64::from(decoder.sample_rate_hz());
                let before = self.pending.len();
                decoder
                    .decode(&c.data, c.frames, &mut self.pending)
                    .map_err(|e| framing(format!("decode error: {}", e)))?;
                if before == 0 && self.pending.len() > before {
                    // A new run: timed from its first kept sample.
                    self.run_start_ns = c.timestamp_ns + skipping * 1_000_000_000 / rate;
                    self.run_frames = 0;
                }
                if self.sequence.is_none() {
                    self.sequence = Some(c.sequence);
                }
                if let Ok(mut a) = self.announced.lock() {
                    a.decoded_chunks += 1;
                }
                let out = self.cut(false)?;
                Ok(Translation::Replace(out))
            }
            Message::StreamEnd(_) | Message::StreamFormat(_) => {
                let out = self.cut(true)?;
                if matches!(m, Message::StreamFormat(_)) {
                    // The next coded chunk opens a decoder for the new format.
                    self.format = None;
                    self.decoder = None;
                    self.sequence = None;
                }
                Ok(if out.is_empty() {
                    Translation::Pass
                } else {
                    Translation::Before(out)
                })
            }
            _ => Ok(Translation::Pass),
        }
    }

    fn ensure_decoder(&mut self) -> io::Result<()> {
        if self.decoder.is_some() {
            return Ok(());
        }
        let format = match self.announced.lock() {
            Ok(a) => a.stream_format.clone(),
            Err(p) => p.into_inner().stream_format.clone(),
        };
        let Some(format) = format else {
            return Err(framing(
                "framing error: a coded chunk arrived with no stream_format announced before it"
                    .to_string(),
            ));
        };
        if format.codec == Codec::Pcm {
            return Err(framing(
                "framing error: the stream was announced as pcm and a coded_chunk arrived"
                    .to_string(),
            ));
        }
        let decoder = Decoder::open(&format).map_err(|e| {
            framing(format!(
                "the {} stream cannot be decoded here: {}",
                format.codec.name(),
                e
            ))
        })?;
        let frame_bytes = decoder.frame_bytes();
        let most = (MAX_PAYLOAD_LEN - CHUNK_HEADER_LEN) / frame_bytes;
        self.chunk_frames = (format.frames_per_chunk as usize).clamp(1, most);
        self.pending.clear();
        self.format = Some(format);
        self.decoder = Some(decoder);
        Ok(())
    }

    /// Cut the pending PCM into `audio_chunk` frames: whole chunks only, or
    /// everything when `all`.
    fn cut(&mut self, all: bool) -> io::Result<Vec<u8>> {
        let (Some(format), Some(decoder)) = (&self.format, &self.decoder) else {
            return Ok(Vec::new());
        };
        let frame_bytes = decoder.frame_bytes();
        let rate = u64::from(format.sample_rate_hz);
        let channels = format.channel_map.len() as u16;
        let sample_format: SampleFormat = format.sample_format;
        let chunk_bytes = self.chunk_frames * frame_bytes;
        let mut out = Vec::new();
        let mut taken = 0;
        while self.pending.len() - taken >= chunk_bytes || (all && self.pending.len() > taken) {
            let n = chunk_bytes.min(self.pending.len() - taken);
            let frames = (n / frame_bytes) as u64;
            let sequence = self.sequence.unwrap_or(0);
            self.sequence = Some(sequence.wrapping_add(1));
            let chunk = AudioChunk {
                sequence,
                timestamp_ns: self.run_start_ns + self.run_frames * 1_000_000_000 / rate,
                sample_rate_hz: format.sample_rate_hz,
                channels,
                sample_format,
                reserved: [0; RESERVED_LEN],
                audio_data: self.pending[taken..taken + n].to_vec(),
            };
            out.extend(
                encode(&Message::AudioChunk(chunk))
                    .map_err(|e| framing(format!("a decoded chunk could not be framed: {}", e)))?,
            );
            self.run_frames += frames;
            taken += n;
        }
        self.pending.drain(..taken);
        Ok(out)
    }
}
