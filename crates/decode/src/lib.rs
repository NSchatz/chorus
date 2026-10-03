//! The server's decoders: compressed media bytes in, PCM out.
//!
//! chorus-server decodes every input format and endpoints only ever receive
//! PCM, FLAC or Opus (proposal P9, K62). This crate is that decode step as a
//! pure library: it reads bytes from a [`Media`], and gives back interleaved
//! `f32` frames at the media's own rate and channel count. It opens no socket
//! and starts no thread; where the bytes come from (a file, an HTTP body) is
//! the caller's.
//!
//! Formats (docs/decoders.md, docs/decisions/0122-the-server-decoders.md): MP3, FLAC,
//! Ogg Vorbis, ALAC in MP4 and WAV/LPCM through Symphonia 0.6.1; Ogg Opus
//! through Symphonia's Ogg demuxer and the vendored libopus 1.6.1
//! (`chorus-opus-sys`, the code the endpoints run); raw big-endian L16 by
//! [`Decoder::open_l16`]. AAC is never compiled and is refused by name
//! (`unsupported: aac`) wherever it shows up.
//!
//! Gapless: encoder delay and padding are trimmed before a frame leaves
//! [`Decoder::read`], so two tracks cut from one recording decode to exactly
//! the recording's frame count (`fixtures/decode`, the gapless pairs).
//!
//! [`Resampler`] and [`remix`] take a decode to the server's stream rate and
//! channel count; the resampler's measured quality is in
//! `docs/measurements/resampler-quality.md`.
#![forbid(unsafe_code)]

mod decoder;
mod media;
mod remix;
mod resample;

pub use decoder::{Codec, DecodeError, Decoder, Format, Hint, Tags};
pub use media::Media;
pub use remix::remix;
pub use resample::Resampler;
