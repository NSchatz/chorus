//! FLAC and Opus for `coded_chunk`: a decoder opened from a `stream_format`
//! and fed one chunk at a time.
//!
//! `docs/protocol.md`, "Codecs: PCM, FLAC and Opus": the stream format carries
//! the codec setup (FLAC: the 34-byte STREAMINFO body; Opus: the OpusHead, RFC
//! 7845 section 5.1) and each chunk ONE FLAC frame or ONE Opus packet with the
//! frames it decodes to. The decoders are the ones proposal P9 settled
//! (`docs/decisions/`, the vendored decoders record): Symphonia's FLAC decoder
//! (MPL-2.0, used unmodified from crates.io), and for Opus the same libopus
//! the endpoint compiles (`chorus-opus-sys`). Output is interleaved
//! little-endian PCM in the stream's `sample_format`, channels in the codec's
//! coded order, which is the interleave order the channel map names. A FLAC
//! stream whose bit depth differs from the output width is shifted to it, as
//! the endpoint does (`firmware/include/chorus/codec.h`). Opus's pre-skip is
//! dropped from the start of the stream and its output gain applied. The
//! endpoint's C decoders and this module read the same fixtures
//! (`fixtures/codec`) and must give the same bytes.

use chorus_protocol::v2::{Codec, StreamFormat};
use chorus_protocol::SampleFormat;
use symphonia_bundle_flac::FlacDecoder;
use symphonia_core::codecs::audio::well_known::CODEC_ID_FLAC;
use symphonia_core::codecs::audio::{AudioCodecParameters, AudioDecoder, AudioDecoderOptions};
use symphonia_core::packet::PacketRef;
use symphonia_core::units::{Duration, Timestamp};

/// The most frames one Opus packet decodes to (120 ms at 48 kHz).
pub const OPUS_MAX_FRAMES: usize = chorus_opus_sys::MAX_FRAMES;

enum Inner {
    Flac {
        decoder: Box<FlacDecoder>,
        /// The STREAMINFO bit depth: where a sample's value sits in its i32.
        bits: u32,
        samples: Vec<i32>,
        planes: Vec<Vec<i32>>,
    },
    Opus {
        decoder: chorus_opus_sys::Decoder,
        s16: Vec<i16>,
        s24: Vec<i32>,
    },
}

/// One stream's decoder.
pub struct Decoder {
    inner: Inner,
    sample_format: SampleFormat,
    channels: usize,
    max_frames: usize,
    skip_left: usize,
    sample_rate_hz: u32,
}

impl std::fmt::Debug for Decoder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Decoder")
            .field("sample_format", &self.sample_format)
            .field("channels", &self.channels)
            .field("max_frames", &self.max_frames)
            .field("skip_left", &self.skip_left)
            .finish()
    }
}

impl Decoder {
    /// A decoder for the stream `format` announces, or the reason there is
    /// none, as a sentence.
    pub fn open(format: &StreamFormat) -> Result<Decoder, String> {
        Decoder::open_checked(format, false)
    }

    /// As [`Decoder::open`], and for FLAC also keep the MD5 of everything
    /// decoded so [`Decoder::md5_matches`] can hold it to the STREAMINFO's
    /// (Symphonia's own verification). For tests: a live stream joined midway
    /// has no whole-stream MD5 to match.
    pub fn open_checked(format: &StreamFormat, verify_md5: bool) -> Result<Decoder, String> {
        let channels = format.channel_map.len();
        let config = &format.codec_config;
        let (inner, max_frames, skip) = match format.codec {
            Codec::Flac => open_flac(format, config, verify_md5)?,
            Codec::Opus => open_opus(format, config)?,
            Codec::Pcm => return Err("a PCM stream has nothing to decode".to_string()),
        };
        Ok(Decoder {
            inner,
            sample_format: format.sample_format,
            channels,
            max_frames,
            skip_left: skip,
            sample_rate_hz: format.sample_rate_hz,
        })
    }

    /// The stream's sample rate.
    pub fn sample_rate_hz(&self) -> u32 {
        self.sample_rate_hz
    }

    /// Bytes per output frame.
    pub fn frame_bytes(&self) -> usize {
        self.channels * bytes_per_sample(self.sample_format)
    }

    /// The most frames one chunk of this stream decodes to.
    pub fn max_frames(&self) -> usize {
        self.max_frames
    }

    /// Opus: the range coder's final state after the last packet. FLAC: 0.
    pub fn final_range(&mut self) -> u32 {
        match &mut self.inner {
            Inner::Opus { decoder, .. } => decoder.final_range(),
            Inner::Flac { .. } => 0,
        }
    }

    /// FLAC opened with the MD5 check: whether the MD5 of everything decoded
    /// equals the STREAMINFO's. `None` for Opus or without the check.
    pub fn md5_matches(&mut self) -> Option<bool> {
        match &mut self.inner {
            Inner::Flac { decoder, .. } => decoder.finalize().verify_ok,
            Inner::Opus { .. } => None,
        }
    }

    /// Frames the Opus pre-skip will still drop.
    pub fn skip_left(&self) -> usize {
        self.skip_left
    }

    /// Decode one chunk (`data`, which the chunk says decodes to `frames`),
    /// appending the PCM to `out`. Returns the frames appended: fewer than
    /// `frames` only while a pre-skip is being dropped.
    pub fn decode(&mut self, data: &[u8], frames: u32, out: &mut Vec<u8>) -> Result<usize, String> {
        let frames = frames as usize;
        if data.is_empty() || frames == 0 {
            return Err("a coded chunk with no data or no frames".to_string());
        }
        if frames > self.max_frames {
            return Err(format!(
                "the chunk claims {} frames and this stream's most is {}",
                frames, self.max_frames
            ));
        }
        let channels = self.channels;
        let format = self.sample_format;
        let (got, values, shift) = match &mut self.inner {
            Inner::Flac {
                decoder,
                bits,
                samples,
                planes,
            } => {
                // Symphonia's FLAC decoder leaves frame integrity to its demuxer,
                // and here there is none: the frame's own CRC-16 (RFC 9639
                // section 9.3, over the whole frame, footer included) is checked
                // first, as dr_flac checks it on the endpoint.
                if data.len() < 4 || data[0] != 0xFF || data[1] & 0xFE != 0xF8 {
                    return Err(format!(
                        "a FLAC chunk of {} bytes does not start with a frame sync code",
                        data.len()
                    ));
                }
                if crc16(data) != 0 {
                    return Err(format!(
                        "a FLAC chunk of {} bytes was refused: its CRC-16 does not match",
                        data.len()
                    ));
                }
                let packet = PacketRef::new(0, Timestamp::ZERO, Duration::new(frames as u64), data);
                let decoded = decoder
                    .decode_ref(&packet)
                    .map_err(|e| format!("a FLAC chunk of {} bytes was refused: {}", data.len(), e))?;
                let got = decoded.frames();
                // Plane i is subframe i, the FLAC coded order (RFC 9639 section
                // 9.1.3), which is the interleave order the channel map names;
                // interleaved here from the planes so no reordering by speaker
                // position can come between.
                planes.clear();
                decoded.copy_to_vecs_planar(planes);
                samples.clear();
                for f in 0..got {
                    for plane in planes.iter() {
                        samples.push(plane[f]);
                    }
                }
                (got, Values::I32(samples), 32 - *bits)
            }
            Inner::Opus { decoder, s16, s24 } => match format {
                SampleFormat::PcmS16Le => {
                    let got = decoder
                        .decode_s16(data, s16)
                        .map_err(|e| format!("libopus refused a packet of {} bytes: {}", data.len(), e))?;
                    (got, Values::I16(&s16[..]), 0)
                }
                _ => {
                    let got = decoder
                        .decode_s24(data, s24)
                        .map_err(|e| format!("libopus refused a packet of {} bytes: {}", data.len(), e))?;
                    (got, Values::I32(&s24[..]), 8)
                }
            },
        };
        if got != frames {
            return Err(format!(
                "the chunk says it decodes to {} frames and it decoded to {}",
                frames, got
            ));
        }
        let skip = self.skip_left.min(got);
        self.skip_left -= skip;
        let from = skip * channels;
        let to = got * channels;
        out.reserve((to - from) * bytes_per_sample(format));
        match values {
            Values::I16(v) => {
                for &s in &v[from..to] {
                    push_sample(out, format, i32::from(s) << 16);
                }
            }
            Values::I32(v) => {
                for &s in &v[from..to] {
                    // Left-justify in 32 bits; a 32-bit FLAC sample already is.
                    let s = if shift >= 32 { 0 } else { ((s as u32) << shift) as i32 };
                    push_sample(out, format, s);
                }
            }
        }
        Ok(got - skip)
    }
}

/// CRC-16 of RFC 9639 section 9.3 (polynomial x^16 + x^15 + x^2 + 1, initial
/// 0); over a whole frame including its footer it is 0.
fn crc16(data: &[u8]) -> u16 {
    let mut crc: u16 = 0;
    for &b in data {
        crc ^= u16::from(b) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 {
                (crc << 1) ^ 0x8005
            } else {
                crc << 1
            };
        }
    }
    crc
}

enum Values<'a> {
    I16(&'a [i16]),
    I32(&'a [i32]),
}

fn bytes_per_sample(format: SampleFormat) -> usize {
    match format {
        SampleFormat::PcmS16Le => 2,
        SampleFormat::PcmS24Le => 3,
        SampleFormat::PcmF32Le => 4,
    }
}

/// Write one sample, given left-justified in 32 bits, in the output format.
fn push_sample(out: &mut Vec<u8>, format: SampleFormat, s: i32) {
    match format {
        SampleFormat::PcmS16Le => out.extend_from_slice(&((s >> 16) as i16).to_le_bytes()),
        SampleFormat::PcmS24Le => out.extend_from_slice(&(s >> 8).to_le_bytes()[..3]),
        // Full scale is 2^31 for the left-justified value.
        SampleFormat::PcmF32Le => {
            out.extend_from_slice(&((s as f32) / 2_147_483_648.0).to_le_bytes())
        }
    }
}

fn open_flac(
    format: &StreamFormat,
    s: &[u8],
    verify_md5: bool,
) -> Result<(Inner, usize, usize), String> {
    if s.len() != 34 {
        return Err(format!(
            "a FLAC codec setup is the 34-byte STREAMINFO body, not {} bytes",
            s.len()
        ));
    }
    if format.sample_format == SampleFormat::PcmF32Le {
        return Err("a FLAC stream is not decoded to pcm_f32le (docs/protocol.md)".to_string());
    }
    // RFC 9639 section 8.2.
    let max_block = usize::from(u16::from_be_bytes([s[2], s[3]]));
    let min_block = usize::from(u16::from_be_bytes([s[0], s[1]]));
    let rate = (u32::from(s[10]) << 12) | (u32::from(s[11]) << 4) | (u32::from(s[12]) >> 4);
    let channels = usize::from((s[12] >> 1) & 0x7) + 1;
    let bits = ((u32::from(s[12] & 1) << 4) | (u32::from(s[13]) >> 4)) + 1;
    if min_block < 16 || max_block < min_block {
        return Err(format!(
            "STREAMINFO block sizes {} to {} are not a FLAC stream's",
            min_block, max_block
        ));
    }
    if rate != format.sample_rate_hz || channels != format.channel_map.len() {
        return Err(format!(
            "STREAMINFO says {} Hz and {} channels and the stream format says {} Hz and {}",
            rate,
            channels,
            format.sample_rate_hz,
            format.channel_map.len()
        ));
    }
    let mut params = AudioCodecParameters::new();
    params
        .for_codec(CODEC_ID_FLAC)
        .with_extra_data(s.to_vec().into_boxed_slice());
    let mut options = AudioDecoderOptions::default();
    options.verify = verify_md5;
    let decoder = FlacDecoder::try_new(&params, &options)
        .map_err(|e| format!("the FLAC decoder refused the STREAMINFO block: {}", e))?;
    Ok((
        Inner::Flac {
            decoder: Box::new(decoder),
            bits,
            samples: Vec::with_capacity(max_block * channels),
            planes: Vec::with_capacity(channels),
        },
        max_block,
        0,
    ))
}

fn open_opus(format: &StreamFormat, h: &[u8]) -> Result<(Inner, usize, usize), String> {
    if h.len() < 19 || &h[..8] != b"OpusHead" {
        return Err("an Opus codec setup is an OpusHead of at least 19 bytes".to_string());
    }
    if h[8] >> 4 != 0 {
        return Err(format!("OpusHead version {} is not one this decoder reads", h[8]));
    }
    let channels = h[9];
    let pre_skip = usize::from(u16::from_le_bytes([h[10], h[11]]));
    let gain = i16::from_le_bytes([h[16], h[17]]);
    let family = h[18];
    if family != 0 {
        return Err(format!(
            "Opus channel mapping family {} is not decoded here yet (family 0 only: one or two \
             channels)",
            family
        ));
    }
    if h.len() != 19 || !(1..=2).contains(&channels) || usize::from(channels) != format.channel_map.len() {
        return Err(format!(
            "the OpusHead ({} bytes, {} channels) does not match a family 0 stream of {} channels",
            h.len(),
            channels,
            format.channel_map.len()
        ));
    }
    if format.sample_rate_hz != 48_000 {
        return Err(format!("an Opus stream is 48000 Hz, not {}", format.sample_rate_hz));
    }
    let decoder = chorus_opus_sys::Decoder::new(channels, gain)
        .map_err(|e| format!("libopus refused the decoder: {}", e))?;
    let n = OPUS_MAX_FRAMES * usize::from(channels);
    Ok((
        Inner::Opus {
            decoder,
            s16: vec![0; n],
            s24: vec![0; n],
        },
        OPUS_MAX_FRAMES,
        pre_skip,
    ))
}
