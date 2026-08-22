//! A decoding session.
//!
//! This phase has no socket, so a session here is not a connection: it is a
//! stateful decoder that outlives any single buffer, counts what it has seen,
//! and stays open across frames it could not use. When a transport arrives it
//! owns one of these per peer and decides for itself when to hang up; the
//! decoder never makes that call. See
//! `docs/decisions/0005-decoder-frame-validation.md`.

use crate::codec::{decode_frame, FrameOutcome};

/// A stateful decoder over a sequence of buffers.
#[derive(Debug, Clone)]
pub struct Session {
    open: bool,
    frames_decoded: u64,
    frames_skipped: u64,
    frames_rejected: u64,
}

impl Default for Session {
    fn default() -> Self {
        Session::new()
    }
}

impl Session {
    /// A new, open session that has seen nothing.
    pub fn new() -> Session {
        Session {
            open: true,
            frames_decoded: 0,
            frames_skipped: 0,
            frames_rejected: 0,
        }
    }

    /// Whether the session is still accepting buffers.
    ///
    /// Nothing a peer can put on the wire changes this. Only [`close`] does.
    ///
    /// [`close`]: Session::close
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Frames that decoded into a catalogued message.
    pub fn frames_decoded(&self) -> u64 {
        self.frames_decoded
    }

    /// Frames skipped because their type byte is not in the catalog.
    pub fn frames_skipped(&self) -> u64 {
        self.frames_skipped
    }

    /// Frames rejected as malformed.
    pub fn frames_rejected(&self) -> u64 {
        self.frames_rejected
    }

    /// Close the session. The caller's decision, never the decoder's.
    pub fn close(&mut self) {
        self.open = false;
    }

    /// Decode every frame in `buf`, in order.
    ///
    /// Returns one outcome per frame reached. Decoding stops early, with the
    /// rejection recorded, when a frame leaves the next boundary unknowable:
    /// a truncated header, or a length field claiming more than the buffer
    /// holds. Stopping is not closing; the next buffer is decoded normally.
    ///
    /// Never panics, and never reads past the end of `buf`.
    pub fn decode_buffer(&mut self, buf: &[u8]) -> Vec<FrameOutcome> {
        let mut outcomes = Vec::new();
        let mut at = 0usize;
        while at < buf.len() {
            let result = decode_frame(&buf[at..]);
            match &result.outcome {
                FrameOutcome::Decoded(_) => self.frames_decoded += 1,
                FrameOutcome::SkippedUnknownType { .. } => self.frames_skipped += 1,
                FrameOutcome::Rejected(_) => self.frames_rejected += 1,
            }
            outcomes.push(result.outcome);
            if result.consumed == 0 {
                break;
            }
            at += result.consumed;
        }
        outcomes
    }
}
