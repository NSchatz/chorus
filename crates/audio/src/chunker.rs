//! Cutting a PCM stream into chunks.
//!
//! A pure function of its configuration and its input. It reads no clock, owns
//! no socket, and holds at most one chunk's worth of bytes, so the same code
//! serves a file, a pipe and a device without knowing which it is.
//!
//! # The end of stream rule
//!
//! Stated once, because every awkward case is a corollary of it:
//!
//! - Every chunk carries exactly the configured duration, **except** the last
//!   chunk of a stream.
//! - The last chunk carries a whole number of frames, greater than zero, no
//!   more than the configured duration, and is marked final.
//! - A trailing remainder that is not a whole frame is discarded, counted in
//!   bytes and reported. It is never padded and never presented as audio.
//! - A whole-frame remainder shorter than one chunk is **not** a discard. It
//!   is the final chunk.
//!
//! So: no whole frame is ever dropped, no chunk is ever padded, and only the
//! final chunk may be short.

use chorus_protocol::{AudioChunk, MAX_PAYLOAD_LEN, RESERVED_LEN};

use crate::format::{StreamFormat, UnsupportedFormat};

/// One chunk the chunker cut, with the two facts the transport needs that the
/// protocol message does not carry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chunk {
    /// The protocol message, ready to encode.
    pub message: AudioChunk,
    /// Whether this is the final chunk of the stream.
    ///
    /// Carried here rather than in the chunk header because
    /// `docs/protocol.md`'s audio chunk layout is committed and its reserved
    /// block is spoken for. End of stream travels as its own message
    /// (`stream_end`), which is what `AC-21` asks for and what an older
    /// decoder skips harmlessly.
    pub is_final: bool,
    /// Frames of audio this chunk carries.
    pub frames: usize,
}

/// What a finished stream did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ChunkerReport {
    /// Chunks emitted, including the final one.
    pub chunks_emitted: u64,
    /// Frames emitted across every chunk.
    pub frames_emitted: u64,
    /// Bytes of a trailing partial frame that were discarded.
    ///
    /// Always smaller than one frame. A whole-frame remainder is emitted as
    /// the final chunk and is not counted here.
    pub bytes_discarded: u64,
}

/// Cuts a byte stream into chunks.
#[derive(Debug, Clone)]
pub struct Chunker {
    format: StreamFormat,
    frames_per_chunk: usize,
    bytes_per_chunk: usize,
    chunk_ns: u64,
    next_sequence: u32,
    next_timestamp_ns: u64,
    pending: Vec<u8>,
    report: ChunkerReport,
    finished: bool,
}

impl Chunker {
    /// A chunker for `format`, cutting `chunk_us` at a time, with the first
    /// chunk stamped `first_timestamp_ns` and numbered `first_sequence`.
    ///
    /// Refuses a chunk duration that is not a whole number of frames, and one
    /// whose payload would not fit a protocol frame. Both are refused here, at
    /// configuration time, rather than at the first chunk.
    pub fn new(
        format: StreamFormat,
        chunk_us: u64,
        first_sequence: u32,
        first_timestamp_ns: u64,
    ) -> Result<Chunker, UnsupportedFormat> {
        let frames_per_chunk = format.frames_in(chunk_us)?;
        let bytes_per_chunk = frames_per_chunk * format.frame_len();
        let needed = chorus_protocol::CHUNK_HEADER_LEN + bytes_per_chunk;
        if needed > MAX_PAYLOAD_LEN {
            return Err(UnsupportedFormat::ChunkTooLarge {
                needed,
                max: MAX_PAYLOAD_LEN,
            });
        }
        Ok(Chunker {
            format,
            frames_per_chunk,
            bytes_per_chunk,
            chunk_ns: chunk_us * 1_000,
            next_sequence: first_sequence,
            next_timestamp_ns: first_timestamp_ns,
            pending: Vec::with_capacity(bytes_per_chunk),
            report: ChunkerReport::default(),
            finished: false,
        })
    }

    /// The stream shape this chunker was built for.
    pub fn format(&self) -> StreamFormat {
        self.format
    }

    /// Frames in a full chunk.
    pub fn frames_per_chunk(&self) -> usize {
        self.frames_per_chunk
    }

    /// Bytes in a full chunk's PCM.
    pub fn bytes_per_chunk(&self) -> usize {
        self.bytes_per_chunk
    }

    /// The configured chunk duration, in nanoseconds.
    pub fn chunk_ns(&self) -> u64 {
        self.chunk_ns
    }

    /// What has happened so far.
    pub fn report(&self) -> ChunkerReport {
        self.report
    }

    /// Feed bytes in, take whole chunks out.
    ///
    /// Returns only full chunks; the remainder is held for the next call or
    /// for [`finish`].
    ///
    /// [`finish`]: Chunker::finish
    pub fn push(&mut self, bytes: &[u8]) -> Vec<Chunk> {
        assert!(!self.finished, "a finished chunker takes no more input");
        let mut out = Vec::new();
        self.pending.extend_from_slice(bytes);
        while self.pending.len() >= self.bytes_per_chunk {
            let rest = self.pending.split_off(self.bytes_per_chunk);
            let pcm = std::mem::replace(&mut self.pending, rest);
            out.push(self.emit(pcm, false));
        }
        out
    }

    /// End the stream.
    ///
    /// Emits the final chunk if a whole frame is still held, and reports any
    /// sub-frame remainder as discarded. Returns `None` when the stream ended
    /// exactly on a chunk boundary with nothing held, in which case the chunk
    /// already emitted was the final one - see [`Chunker::last_emitted_final`].
    pub fn finish(&mut self) -> (Option<Chunk>, ChunkerReport) {
        assert!(!self.finished, "a chunker is finished once");
        self.finished = true;
        let frame_len = self.format.frame_len();
        let held = self.pending.len();
        let whole_frames = held / frame_len;
        let remainder = held % frame_len;
        self.report.bytes_discarded += remainder as u64;

        if whole_frames == 0 {
            self.pending.clear();
            return (None, self.report);
        }

        let mut pcm = std::mem::take(&mut self.pending);
        pcm.truncate(whole_frames * frame_len);
        let chunk = self.emit(pcm, true);
        (Some(chunk), self.report)
    }

    /// Whether the stream ended exactly on a chunk boundary.
    ///
    /// When it did, [`finish`] returns no chunk and the caller has to mark the
    /// chunk it already emitted as the final one. The caller knows which that
    /// was; the chunker does not keep it.
    ///
    /// [`finish`]: Chunker::finish
    pub fn last_emitted_final(&self) -> bool {
        self.finished && self.pending.is_empty() && self.report.chunks_emitted > 0
    }

    fn emit(&mut self, pcm: Vec<u8>, is_final: bool) -> Chunk {
        let frames = pcm.len() / self.format.frame_len();
        let message = AudioChunk {
            sequence: self.next_sequence,
            timestamp_ns: self.next_timestamp_ns,
            sample_rate_hz: self.format.sample_rate_hz,
            channels: self.format.channels,
            sample_format: self.format.sample_format,
            reserved: [0u8; RESERVED_LEN],
            audio_data: pcm,
        };
        // Sequence increases by exactly one, and wraps, which docs/protocol.md
        // allows for. Timestamps advance by the configured chunk duration
        // start to start, so a short final chunk leaves the spacing alone.
        self.next_sequence = self.next_sequence.wrapping_add(1);
        self.next_timestamp_ns = self.next_timestamp_ns.saturating_add(self.chunk_ns);
        self.report.chunks_emitted += 1;
        self.report.frames_emitted += frames as u64;
        Chunk {
            message,
            is_final,
            frames,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stereo_48k() -> StreamFormat {
        StreamFormat::new(48_000, 2, "pcm_s16le").unwrap()
    }

    /// Bytes that are recognisable at any offset, so a reordering or a drop is
    /// visible in the concatenation rather than invisible.
    fn ramp(len: usize) -> Vec<u8> {
        (0..len).map(|i| (i % 251) as u8).collect()
    }

    #[test]
    fn every_chunk_but_the_last_carries_the_configured_duration() {
        let format = stereo_48k();
        let mut chunker = Chunker::new(format, 20_000, 0, 0).unwrap();
        // Ten full chunks plus 100 frames.
        let input = ramp(chunker.bytes_per_chunk() * 10 + 100 * format.frame_len());
        let mut chunks = chunker.push(&input);
        let (last, report) = chunker.finish();
        chunks.push(last.expect("a whole-frame remainder is the final chunk"));

        assert_eq!(chunks.len(), 11);
        for c in &chunks[..10] {
            assert_eq!(c.frames, 960);
            assert!(!c.is_final);
        }
        assert_eq!(chunks[10].frames, 100);
        assert!(chunks[10].is_final);
        assert_eq!(report.bytes_discarded, 0);
        assert_eq!(report.chunks_emitted, 11);
    }

    #[test]
    fn the_concatenation_is_the_input_byte_for_byte() {
        let format = stereo_48k();
        let mut chunker = Chunker::new(format, 20_000, 0, 0).unwrap();
        let input = ramp(chunker.bytes_per_chunk() * 7 + 13 * format.frame_len());
        let mut out = Vec::new();
        for c in chunker.push(&input) {
            out.extend_from_slice(&c.message.audio_data);
        }
        let (last, report) = chunker.finish();
        out.extend_from_slice(&last.unwrap().message.audio_data);
        assert_eq!(out, input);
        assert_eq!(report.bytes_discarded, 0);
    }

    #[test]
    fn a_sub_frame_remainder_is_discarded_and_counted_not_padded() {
        let format = stereo_48k();
        let mut chunker = Chunker::new(format, 20_000, 0, 0).unwrap();
        let body = chunker.bytes_per_chunk() * 10 + 100 * format.frame_len();
        let input = ramp(body + 3);
        let mut chunks = chunker.push(&input);
        let (last, report) = chunker.finish();
        chunks.push(last.unwrap());

        assert_eq!(chunks.len(), 11);
        assert_eq!(chunks[10].frames, 100);
        assert!(chunks[10].is_final);
        assert_eq!(report.bytes_discarded, 3);

        let mut out = Vec::new();
        for c in &chunks {
            out.extend_from_slice(&c.message.audio_data);
        }
        assert_eq!(out, input[..body], "no padding, and nothing whole lost");
    }

    #[test]
    fn one_frame_plus_three_bytes_is_one_final_chunk_and_three_bytes() {
        let format = stereo_48k();
        let mut chunker = Chunker::new(format, 20_000, 0, 0).unwrap();
        let input = ramp(format.frame_len() + 3);
        let chunks = chunker.push(&input);
        assert!(chunks.is_empty());
        let (last, report) = chunker.finish();
        let last = last.unwrap();
        assert_eq!(last.frames, 1);
        assert!(last.is_final);
        assert_eq!(report.bytes_discarded, 3);
        assert_eq!(report.chunks_emitted, 1);
        assert_eq!(last.message.sequence, 0);
    }

    #[test]
    fn sequence_numbers_are_a_contiguous_run() {
        let format = stereo_48k();
        let mut chunker = Chunker::new(format, 20_000, 0, 0).unwrap();
        // Three bytes is less than one four-byte frame, so nothing is held
        // back as a final chunk and the run of full chunks is the whole run.
        assert_eq!(format.frame_len(), 4);
        let input = ramp(chunker.bytes_per_chunk() * 25 + 3);
        let chunks = chunker.push(&input);
        let (last, report) = chunker.finish();
        assert!(last.is_none());
        assert_eq!(report.bytes_discarded, 3);
        assert!(chunker.last_emitted_final());
        assert_eq!(chunks.len(), 25);
        for (i, c) in chunks.iter().enumerate() {
            assert_eq!(c.message.sequence, i as u32);
        }
    }

    #[test]
    fn timestamps_advance_by_exactly_the_chunk_duration_including_the_short_one() {
        let format = stereo_48k();
        let mut chunker = Chunker::new(format, 20_000, 0, 5_000).unwrap();
        let input = ramp(chunker.bytes_per_chunk() * 3 + 40 * format.frame_len());
        let mut chunks = chunker.push(&input);
        let (last, _) = chunker.finish();
        chunks.push(last.unwrap());

        let stamps: Vec<u64> = chunks.iter().map(|c| c.message.timestamp_ns).collect();
        assert_eq!(stamps, vec![5_000, 20_005_000, 40_005_000, 60_005_000]);
        for w in stamps.windows(2) {
            assert!(w[1] > w[0], "strictly increasing");
            assert_eq!(w[1] - w[0], 20_000_000, "constant delta, start to start");
        }
    }

    #[test]
    fn feeding_in_arbitrary_pieces_produces_the_same_chunks() {
        let format = stereo_48k();
        let mut whole = Chunker::new(format, 20_000, 0, 0).unwrap();
        let mut piecemeal = Chunker::new(format, 20_000, 0, 0).unwrap();
        let input = ramp(whole.bytes_per_chunk() * 4 + 999);

        let a: Vec<Chunk> = whole.push(&input);
        let mut b: Vec<Chunk> = Vec::new();
        for piece in input.chunks(37) {
            b.extend(piecemeal.push(piece));
        }
        assert_eq!(a, b);
        let (fa, ra) = whole.finish();
        let (fb, rb) = piecemeal.finish();
        assert_eq!(fa, fb);
        assert_eq!(ra, rb);
    }

    #[test]
    fn a_chunk_that_cannot_fit_a_frame_payload_is_refused_at_configuration() {
        let format = StreamFormat::new(192_000, 8, "pcm_f32le").unwrap();
        // 100 ms at 192 kHz, 8 channels, 4 bytes: far past 65535.
        let err = Chunker::new(format, 100_000, 0, 0).unwrap_err();
        assert!(matches!(err, UnsupportedFormat::ChunkTooLarge { .. }));
    }

    #[test]
    fn an_empty_stream_emits_nothing_and_discards_nothing() {
        let mut chunker = Chunker::new(stereo_48k(), 20_000, 0, 0).unwrap();
        assert!(chunker.push(&[]).is_empty());
        let (last, report) = chunker.finish();
        assert!(last.is_none());
        assert_eq!(report, ChunkerReport::default());
    }
}
