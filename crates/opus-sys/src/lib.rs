//! libopus 1.6.1, vendored in `third_party/opus` and compiled by `build.rs`
//! exactly as the endpoint compiles it (fixed point, 24-bit internal
//! resolution, no float API, decoder units only), and a safe decoder over it.
//!
//! This is the one place in the Linux client where Rust calls C: the same
//! Opus code decodes on the endpoint and here, which is proposal P9's own
//! criterion ("Same Opus code as the endpoint: Yes") and what the shared
//! fixtures in `fixtures/codec` hold both sides to. The functions used are
//! libopus's documented API (`include/opus.h`, `include/opus_defines.h`).
#![allow(
    unsafe_code,
    reason = "the crate is the FFI binding to the vendored libopus"
)]

use std::ffi::{c_char, c_int, CStr};
use std::fmt;
use std::ptr::NonNull;

/// libopus's opaque decoder state.
#[repr(C)]
struct OpusDecoder {
    _private: [u8; 0],
}

const OPUS_OK: c_int = 0;
// include/opus_defines.h: the request numbers of the two controls used here.
const OPUS_SET_GAIN_REQUEST: c_int = 4034;
const OPUS_GET_FINAL_RANGE_REQUEST: c_int = 4031;

extern "C" {
    fn opus_decoder_create(fs: i32, channels: c_int, error: *mut c_int) -> *mut OpusDecoder;
    fn opus_decoder_destroy(st: *mut OpusDecoder);
    fn opus_decode(
        st: *mut OpusDecoder,
        data: *const u8,
        len: i32,
        pcm: *mut i16,
        frame_size: c_int,
        decode_fec: c_int,
    ) -> c_int;
    fn opus_decode24(
        st: *mut OpusDecoder,
        data: *const u8,
        len: i32,
        pcm: *mut i32,
        frame_size: c_int,
        decode_fec: c_int,
    ) -> c_int;
    fn opus_decoder_ctl(st: *mut OpusDecoder, request: c_int, ...) -> c_int;
    fn opus_strerror(error: c_int) -> *const c_char;
    fn chorus_opus_compare_main(argc: c_int, argv: *const *const c_char) -> c_int;
}

/// The most frames one packet decodes to at 48 kHz: 120 ms (RFC 6716
/// section 3.2.5).
pub const MAX_FRAMES: usize = 5760;

/// A libopus error, by its code and libopus's own words for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpusError {
    /// The negative code libopus returned.
    pub code: i32,
}

impl fmt::Display for OpusError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // SAFETY: opus_strerror returns a pointer to a static, NUL-terminated
        // string for every code, known or not (src/opus.c).
        let text = unsafe { CStr::from_ptr(opus_strerror(self.code)) };
        write!(f, "{} ({})", text.to_string_lossy(), self.code)
    }
}

impl std::error::Error for OpusError {}

/// One Opus decoder at 48 kHz, one or two channels (mapping family 0).
pub struct Decoder {
    state: NonNull<OpusDecoder>,
    channels: usize,
}

// SAFETY: the decoder state is plain memory owned by this value and touched
// only through `&mut self`; libopus keeps no thread-local or global state for
// it.
unsafe impl Send for Decoder {}

impl Decoder {
    /// A decoder for `channels` (1 or 2) at 48 kHz, applying `gain_q8` (the
    /// OpusHead output gain, Q7.8 dB) through libopus's `OPUS_SET_GAIN`.
    pub fn new(channels: u8, gain_q8: i16) -> Result<Decoder, OpusError> {
        let mut error: c_int = OPUS_OK;
        // SAFETY: `error` is a valid out pointer for the duration of the call.
        let raw = unsafe { opus_decoder_create(48_000, c_int::from(channels), &mut error) };
        let state = match NonNull::new(raw) {
            Some(s) if error == OPUS_OK => s,
            _ => return Err(OpusError { code: error }),
        };
        let decoder = Decoder {
            state,
            channels: usize::from(channels),
        };
        if gain_q8 != 0 {
            // SAFETY: the state is live; OPUS_SET_GAIN takes one opus_int32 by value.
            let r = unsafe {
                opus_decoder_ctl(
                    decoder.state.as_ptr(),
                    OPUS_SET_GAIN_REQUEST,
                    i32::from(gain_q8),
                )
            };
            if r != OPUS_OK {
                return Err(OpusError { code: r });
            }
        }
        Ok(decoder)
    }

    /// Decode one packet to 16-bit samples, interleaved; returns the frames.
    /// `pcm` must hold [`MAX_FRAMES`] times the channel count.
    pub fn decode_s16(&mut self, packet: &[u8], pcm: &mut [i16]) -> Result<usize, OpusError> {
        let frames = pcm.len() / self.channels;
        let len = i32::try_from(packet.len()).map_err(|_| OpusError { code: -1 })?;
        // SAFETY: `packet` is `len` readable bytes; `pcm` holds `frames` frames of
        // `channels` samples, which is what frame_size tells libopus it may write.
        let n = unsafe {
            opus_decode(
                self.state.as_ptr(),
                packet.as_ptr(),
                len,
                pcm.as_mut_ptr(),
                frames as c_int,
                0,
            )
        };
        if n < 0 {
            return Err(OpusError { code: n });
        }
        Ok(n as usize)
    }

    /// Decode one packet to 24-bit samples in 32-bit words, interleaved.
    pub fn decode_s24(&mut self, packet: &[u8], pcm: &mut [i32]) -> Result<usize, OpusError> {
        let frames = pcm.len() / self.channels;
        let len = i32::try_from(packet.len()).map_err(|_| OpusError { code: -1 })?;
        // SAFETY: as in decode_s16, with 32-bit output words.
        let n = unsafe {
            opus_decode24(
                self.state.as_ptr(),
                packet.as_ptr(),
                len,
                pcm.as_mut_ptr(),
                frames as c_int,
                0,
            )
        };
        if n < 0 {
            return Err(OpusError { code: n });
        }
        Ok(n as usize)
    }

    /// The range coder's final state after the last packet
    /// (`OPUS_GET_FINAL_RANGE`, RFC 6716 section 4.1.6).
    pub fn final_range(&mut self) -> u32 {
        let mut range: u32 = 0;
        // SAFETY: the state is live; OPUS_GET_FINAL_RANGE takes one opus_uint32
        // out pointer, valid for the call.
        let r = unsafe {
            opus_decoder_ctl(
                self.state.as_ptr(),
                OPUS_GET_FINAL_RANGE_REQUEST,
                &mut range as *mut u32,
            )
        };
        if r == OPUS_OK {
            range
        } else {
            0
        }
    }
}

impl Drop for Decoder {
    fn drop(&mut self) {
        // SAFETY: the state came from opus_decoder_create and is destroyed once.
        unsafe { opus_decoder_destroy(self.state.as_ptr()) }
    }
}

/// Run libopus's conformance tool, `opus_compare` (src/opus_compare.c,
/// vendored unchanged), on a reference decode and a decode, both files of
/// 16-bit little-endian PCM at 48 kHz: the reference always two channels, the
/// decode two when `stereo` and one otherwise. Returns whether it passes; the
/// tool prints its verdict and quality figure on standard error.
pub fn opus_compare(reference: &str, decoded: &str, stereo: bool) -> bool {
    let program = c"opus_compare";
    let flag = c"-s";
    let reference = std::ffi::CString::new(reference).expect("a path without NUL");
    let decoded = std::ffi::CString::new(decoded).expect("a path without NUL");
    let mut argv = vec![program.as_ptr()];
    if stereo {
        argv.push(flag.as_ptr());
    }
    argv.push(reference.as_ptr());
    argv.push(decoded.as_ptr());
    // SAFETY: argv holds argc valid NUL-terminated strings that outlive the
    // call; the tool reads them and the two files and returns a status.
    let status = unsafe { chorus_opus_compare_main(argv.len() as c_int, argv.as_ptr()) };
    status == 0
}
