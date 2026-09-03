//! ALSA PCM playback for the Linux client.
//!
//! This is the one place in the repository that touches an audio device. It is
//! deliberately small: open a playback device, hand it interleaved PCM, ask it
//! how far the frames it already holds are from the DAC, and notice when it
//! ran dry.
//!
//! # Why the library is loaded at run time rather than linked
//!
//! `docs/decisions/0002-repository-layout-and-ci.md` records that this
//! repository has zero external dependencies and no lockfile, and that landing
//! the first one is a decision-log entry rather than a quiet change. The
//! roadmap phase this crate belongs to names ALSA as the output path, so ALSA
//! itself is not a choice; the choice is how to reach it. Binding
//! `libasound.so.2` with `dlopen` at run time keeps the build exactly where it
//! was: no crate, no `-dev` package, no build script, and `cargo build` works
//! on a machine with no audio stack at all. The cost is that a missing or
//! unusable ALSA runtime is a start-up error rather than a link error, which
//! is the behaviour the client wants anyway.
//!
//! The reasoning is written up in
//! `docs/decisions/0008-alsa-binding-and-the-audio-sink.md`.
//!
//! # What is on the audio path
//!
//! Everything here. No function in this crate reads a settable wall clock; the
//! only clock it consults is the device's own frame counter, through
//! `snd_pcm_delay`.

#![warn(missing_docs)]

use std::ffi::{CStr, CString};
use std::fmt;
use std::os::raw::{c_char, c_int, c_long, c_uint, c_ulong, c_void};
use std::sync::OnceLock;

/// Soname this crate loads. Loading the versioned soname rather than the
/// `-dev` symlink is what lets a machine without ALSA headers still run the
/// client.
pub const LIBASOUND_SONAME: &str = "libasound.so.2";

/// `SND_PCM_STREAM_PLAYBACK`.
const STREAM_PLAYBACK: c_int = 0;

/// `SND_PCM_STREAM_CAPTURE`.
///
/// The measurement harness records two endpoint line outputs through one
/// interface, which is the one thing this crate is asked for that playback
/// alone cannot give. It reaches the device through the same `dlopen`ed
/// library and the same handful of entry points, so a machine with no audio
/// stack still builds and still starts; it simply refuses at the point the
/// device is opened.
const STREAM_CAPTURE: c_int = 1;

/// `SND_PCM_ACCESS_RW_INTERLEAVED`.
const ACCESS_RW_INTERLEAVED: c_int = 3;

/// `SND_PCM_STATE_XRUN`.
const STATE_XRUN: c_int = 4;

/// `SND_PCM_STATE_DISCONNECTED`.
const STATE_DISCONNECTED: c_int = 8;

/// `-EPIPE`, which is what ALSA returns for a playback underrun.
const NEG_EPIPE: c_int = -32;

/// `-ENODEV`, returned once a device has gone away under a running stream.
const NEG_ENODEV: c_int = -19;

/// `RTLD_NOW | RTLD_LOCAL`.
const RTLD_NOW: c_int = 2;

extern "C" {
    fn dlopen(filename: *const c_char, flags: c_int) -> *mut c_void;
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
    fn dlerror() -> *mut c_char;
}

type SndPcmOpen = unsafe extern "C" fn(*mut *mut c_void, *const c_char, c_int, c_int) -> c_int;
type SndPcmClose = unsafe extern "C" fn(*mut c_void) -> c_int;
type SndPcmSetParams =
    unsafe extern "C" fn(*mut c_void, c_int, c_int, c_uint, c_uint, c_int, c_uint) -> c_int;
type SndPcmWritei = unsafe extern "C" fn(*mut c_void, *const c_void, c_ulong) -> c_long;
type SndPcmReadi = unsafe extern "C" fn(*mut c_void, *mut c_void, c_ulong) -> c_long;
type SndPcmDelay = unsafe extern "C" fn(*mut c_void, *mut c_long) -> c_int;
type SndPcmPrepare = unsafe extern "C" fn(*mut c_void) -> c_int;
type SndPcmRecover = unsafe extern "C" fn(*mut c_void, c_int, c_int) -> c_int;
type SndPcmDrain = unsafe extern "C" fn(*mut c_void) -> c_int;
type SndPcmState = unsafe extern "C" fn(*mut c_void) -> c_int;
type SndStrerror = unsafe extern "C" fn(c_int) -> *const c_char;

/// The handful of `libasound` entry points this client needs.
///
/// Only function pointers are kept. The `dlopen` handle is deliberately
/// dropped: the library stays mapped for the life of the process, which is
/// what the client wants, and not holding the handle keeps this type `Send`
/// and `Sync` without an unsafe promise.
struct Lib {
    open: SndPcmOpen,
    close: SndPcmClose,
    set_params: SndPcmSetParams,
    writei: SndPcmWritei,
    readi: SndPcmReadi,
    delay: SndPcmDelay,
    prepare: SndPcmPrepare,
    recover: SndPcmRecover,
    drain: SndPcmDrain,
    state: SndPcmState,
    strerror: SndStrerror,
}

/// Why an audio device could not be used.
///
/// Every variant names the device and enough of the reason that a run which
/// exits on one of these says what an operator has to change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AlsaError {
    /// `libasound.so.2` is not present, or could not be loaded.
    RuntimeMissing {
        /// The soname that was looked for.
        soname: String,
        /// What the dynamic loader said.
        detail: String,
    },
    /// The library loaded and a symbol this client needs is not in it.
    SymbolMissing {
        /// The symbol that was looked for.
        symbol: String,
    },
    /// A `libasound` call failed.
    Call {
        /// Which call.
        call: &'static str,
        /// The device the call was about.
        device: String,
        /// The negative errno ALSA returned.
        code: c_int,
        /// `snd_strerror` of that code.
        detail: String,
    },
    /// A format this crate cannot ask ALSA for.
    UnsupportedFormat {
        /// The format's protocol name.
        format: String,
    },
    /// A device name that cannot be handed to a C API.
    DeviceNameNotRepresentable {
        /// The name that was configured.
        device: String,
    },
    /// The device went away under a running stream.
    Disconnected {
        /// The device that went away.
        device: String,
    },
}

impl fmt::Display for AlsaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AlsaError::RuntimeMissing { soname, detail } => write!(
                f,
                "the ALSA runtime {} could not be loaded: {}",
                soname, detail
            ),
            AlsaError::SymbolMissing { symbol } => write!(
                f,
                "the ALSA runtime does not export {}, which this client needs",
                symbol
            ),
            AlsaError::Call {
                call,
                device,
                code,
                detail,
            } => write!(
                f,
                "{} on audio device '{}' failed: {} (errno {})",
                call, device, detail, -code
            ),
            AlsaError::UnsupportedFormat { format } => {
                write!(f, "no ALSA format carries chorus format '{}'", format)
            }
            AlsaError::DeviceNameNotRepresentable { device } => write!(
                f,
                "audio device name '{}' cannot be passed to ALSA (embedded NUL)",
                device
            ),
            AlsaError::Disconnected { device } => {
                write!(f, "audio device '{}' disconnected", device)
            }
        }
    }
}

impl std::error::Error for AlsaError {}

/// The PCM layouts this sink can ask ALSA for.
///
/// The three values mirror `chorus_protocol::SampleFormat` without depending
/// on it: this crate stays a leaf so that the protocol library keeps its
/// `forbid(unsafe_code)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// Signed 16-bit little-endian, `SND_PCM_FORMAT_S16_LE`.
    S16Le,
    /// Signed 24-bit little-endian packed in three bytes,
    /// `SND_PCM_FORMAT_S24_3LE`. The packed layout is the one
    /// `docs/protocol.md` defines, not ALSA's four-byte `S24_LE`.
    S24Packed3Le,
    /// 32-bit float little-endian, `SND_PCM_FORMAT_FLOAT_LE`.
    F32Le,
}

impl Format {
    /// The `snd_pcm_format_t` value for this layout.
    fn to_alsa(self) -> c_int {
        match self {
            Format::S16Le => 2,
            Format::S24Packed3Le => 32,
            Format::F32Le => 14,
        }
    }

    /// Bytes one sample of one channel occupies.
    pub fn bytes_per_sample(self) -> usize {
        match self {
            Format::S16Le => 2,
            Format::S24Packed3Le => 3,
            Format::F32Le => 4,
        }
    }

    /// The protocol name for this layout.
    pub fn name(self) -> &'static str {
        match self {
            Format::S16Le => "pcm_s16le",
            Format::S24Packed3Le => "pcm_s24le",
            Format::F32Le => "pcm_f32le",
        }
    }
}

fn last_dl_error() -> String {
    // SAFETY: dlerror returns either NUL or a pointer to a NUL-terminated
    // string owned by the loader, valid until the next dlerror call.
    unsafe {
        let raw = dlerror();
        if raw.is_null() {
            "no detail from the dynamic loader".to_string()
        } else {
            CStr::from_ptr(raw).to_string_lossy().into_owned()
        }
    }
}

fn load_symbol(handle: *mut c_void, name: &str) -> Result<*mut c_void, AlsaError> {
    let c_name = CString::new(name).expect("symbol names in this crate are literals without NUL");
    // SAFETY: handle came from a successful dlopen and c_name is NUL
    // terminated for the duration of the call.
    let sym = unsafe {
        let _ = dlerror();
        dlsym(handle, c_name.as_ptr())
    };
    if sym.is_null() {
        return Err(AlsaError::SymbolMissing {
            symbol: name.to_string(),
        });
    }
    Ok(sym)
}

fn load_lib() -> Result<Lib, AlsaError> {
    let soname = CString::new(LIBASOUND_SONAME).expect("the soname is a literal without NUL");
    // SAFETY: soname is NUL terminated and lives across the call.
    let handle = unsafe {
        let _ = dlerror();
        dlopen(soname.as_ptr(), RTLD_NOW)
    };
    if handle.is_null() {
        return Err(AlsaError::RuntimeMissing {
            soname: LIBASOUND_SONAME.to_string(),
            detail: last_dl_error(),
        });
    }

    // SAFETY: every symbol below is looked up by its documented name in
    // libasound and transmuted to the signature `alsa/pcm.h` declares for it.
    // A wrong signature here would be undefined behaviour, so each one is
    // written out rather than generated.
    unsafe {
        Ok(Lib {
            open: std::mem::transmute::<*mut c_void, SndPcmOpen>(load_symbol(
                handle,
                "snd_pcm_open",
            )?),
            close: std::mem::transmute::<*mut c_void, SndPcmClose>(load_symbol(
                handle,
                "snd_pcm_close",
            )?),
            set_params: std::mem::transmute::<*mut c_void, SndPcmSetParams>(load_symbol(
                handle,
                "snd_pcm_set_params",
            )?),
            writei: std::mem::transmute::<*mut c_void, SndPcmWritei>(load_symbol(
                handle,
                "snd_pcm_writei",
            )?),
            readi: std::mem::transmute::<*mut c_void, SndPcmReadi>(load_symbol(
                handle,
                "snd_pcm_readi",
            )?),
            delay: std::mem::transmute::<*mut c_void, SndPcmDelay>(load_symbol(
                handle,
                "snd_pcm_delay",
            )?),
            prepare: std::mem::transmute::<*mut c_void, SndPcmPrepare>(load_symbol(
                handle,
                "snd_pcm_prepare",
            )?),
            recover: std::mem::transmute::<*mut c_void, SndPcmRecover>(load_symbol(
                handle,
                "snd_pcm_recover",
            )?),
            drain: std::mem::transmute::<*mut c_void, SndPcmDrain>(load_symbol(
                handle,
                "snd_pcm_drain",
            )?),
            state: std::mem::transmute::<*mut c_void, SndPcmState>(load_symbol(
                handle,
                "snd_pcm_state",
            )?),
            strerror: std::mem::transmute::<*mut c_void, SndStrerror>(load_symbol(
                handle,
                "snd_strerror",
            )?),
        })
    }
}

fn lib() -> Result<&'static Lib, AlsaError> {
    static LIB: OnceLock<Result<Lib, AlsaError>> = OnceLock::new();
    match LIB.get_or_init(load_lib) {
        Ok(l) => Ok(l),
        Err(e) => Err(e.clone()),
    }
}

/// Whether the ALSA runtime this client needs is present and complete.
///
/// Separated from opening a device so a prerequisite check can tell "there is
/// no ALSA on this machine" from "this device does not exist", which are
/// different things to tell an operator.
pub fn runtime_available() -> Result<(), AlsaError> {
    lib().map(|_| ())
}

/// What the device reported when frames were handed to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WriteReport {
    /// Frames the device accepted.
    pub frames_written: u64,
    /// Whether the device signalled an underrun that this write recovered
    /// from.
    ///
    /// This is the device's own signal (`-EPIPE`, or a PCM state of
    /// `SND_PCM_STATE_XRUN`), never an inference from the reported delay,
    /// which `alsa` documents does not necessarily fall to zero on underrun.
    pub underran: bool,
}

/// An open playback device.
pub struct Pcm {
    handle: *mut c_void,
    device: String,
    format: Format,
    channels: u16,
    rate_hz: u32,
    closed: bool,
}

// SAFETY: a Pcm owns its handle exclusively. The client moves one into the
// playout thread and never shares it; nothing here is Sync.
unsafe impl Send for Pcm {}

impl Pcm {
    /// Open `device` for playback and configure it for this stream.
    ///
    /// `buffer_us` is the ring the device is asked for; the client sizes it
    /// above its own configured maximum so that the reported delay can reach
    /// that maximum without the ring being the thing that caps it.
    pub fn open(
        device: &str,
        format: Format,
        channels: u16,
        rate_hz: u32,
        buffer_us: u32,
    ) -> Result<Pcm, AlsaError> {
        Pcm::open_stream(device, STREAM_PLAYBACK, format, channels, rate_hz, buffer_us)
    }

    /// Open `device` for capture and configure it for this stream.
    ///
    /// The measurement harness's only use of a device: two endpoint line
    /// outputs arrive as the two channels of one interface, and the harness
    /// reads them. Everything else about this crate is unchanged, including
    /// that the library is loaded at run time, so a machine with no audio stack
    /// still builds and still starts.
    pub fn open_capture(
        device: &str,
        format: Format,
        channels: u16,
        rate_hz: u32,
        buffer_us: u32,
    ) -> Result<Pcm, AlsaError> {
        Pcm::open_stream(device, STREAM_CAPTURE, format, channels, rate_hz, buffer_us)
    }

    fn open_stream(
        device: &str,
        stream: c_int,
        format: Format,
        channels: u16,
        rate_hz: u32,
        buffer_us: u32,
    ) -> Result<Pcm, AlsaError> {
        let lib = lib()?;
        let c_device =
            CString::new(device).map_err(|_| AlsaError::DeviceNameNotRepresentable {
                device: device.to_string(),
            })?;

        let mut handle: *mut c_void = std::ptr::null_mut();
        // SAFETY: c_device outlives the call; ALSA copies the name.
        let rc = unsafe { (lib.open)(&mut handle, c_device.as_ptr(), stream, 0) };
        if rc < 0 || handle.is_null() {
            return Err(Pcm::error(lib, "snd_pcm_open", device, rc));
        }

        let mut pcm = Pcm {
            handle,
            device: device.to_string(),
            format,
            channels,
            rate_hz,
            closed: false,
        };

        // SAFETY: handle is a live snd_pcm_t from the open above.
        let rc = unsafe {
            (lib.set_params)(
                pcm.handle,
                format.to_alsa(),
                ACCESS_RW_INTERLEAVED,
                c_uint::from(channels),
                rate_hz as c_uint,
                0,
                buffer_us as c_uint,
            )
        };
        if rc < 0 {
            let err = Pcm::error(lib, "snd_pcm_set_params", device, rc);
            pcm.close_inner();
            return Err(err);
        }

        // SAFETY: handle is live and configured.
        let rc = unsafe { (lib.prepare)(pcm.handle) };
        if rc < 0 {
            let err = Pcm::error(lib, "snd_pcm_prepare", device, rc);
            pcm.close_inner();
            return Err(err);
        }

        Ok(pcm)
    }

    fn error(lib: &Lib, call: &'static str, device: &str, code: c_int) -> AlsaError {
        // SAFETY: snd_strerror returns a static NUL-terminated string.
        let detail = unsafe {
            let raw = (lib.strerror)(code);
            if raw.is_null() {
                String::from("no detail")
            } else {
                CStr::from_ptr(raw).to_string_lossy().into_owned()
            }
        };
        AlsaError::Call {
            call,
            device: device.to_string(),
            code,
            detail,
        }
    }

    /// The device name this stream was opened on.
    pub fn device(&self) -> &str {
        &self.device
    }

    /// Bytes one frame occupies on this stream.
    pub fn frame_len(&self) -> usize {
        self.channels as usize * self.format.bytes_per_sample()
    }

    /// Sample rate this stream was opened at.
    pub fn rate_hz(&self) -> u32 {
        self.rate_hz
    }

    /// Hand `pcm` to the device, blocking until every frame is accepted.
    ///
    /// Recovers from an underrun and keeps going rather than failing the run:
    /// the count is what the client reports, and dropping the stream on the
    /// first underrun would make the counter unobservable.
    pub fn write(&mut self, pcm: &[u8]) -> Result<WriteReport, AlsaError> {
        let lib = lib()?;
        let frame_len = self.frame_len();
        debug_assert!(frame_len > 0);
        if pcm.len() % frame_len != 0 {
            // The caller is the client's own playout loop, which only ever
            // hands whole frames; a partial frame here is a bug, not input.
            debug_assert!(false, "partial frame handed to the sink");
        }

        let mut offset = 0usize;
        let mut frames_written = 0u64;
        let mut underran = false;

        while offset < pcm.len() {
            let frames_left = ((pcm.len() - offset) / frame_len) as c_ulong;
            if frames_left == 0 {
                break;
            }
            // SAFETY: the slice is live for the call and describes exactly
            // frames_left frames of the configured format.
            let rc = unsafe {
                (lib.writei)(
                    self.handle,
                    pcm[offset..].as_ptr() as *const c_void,
                    frames_left,
                )
            };
            if rc >= 0 {
                let n = rc as usize;
                if n == 0 {
                    // No progress and no error: treat as a stalled device
                    // rather than spinning forever.
                    return Err(Pcm::error(lib, "snd_pcm_writei", &self.device, NEG_EPIPE));
                }
                offset += n * frame_len;
                frames_written += n as u64;
                continue;
            }

            let code = rc as c_int;
            if code == NEG_ENODEV {
                return Err(AlsaError::Disconnected {
                    device: self.device.clone(),
                });
            }
            if code == NEG_EPIPE {
                underran = true;
            }
            // SAFETY: handle is live; silent recovery, the client does the
            // reporting.
            let recovered = unsafe { (lib.recover)(self.handle, code, 1) };
            if recovered < 0 {
                if recovered as c_int == NEG_ENODEV {
                    return Err(AlsaError::Disconnected {
                        device: self.device.clone(),
                    });
                }
                return Err(Pcm::error(lib, "snd_pcm_writei", &self.device, code));
            }
        }

        Ok(WriteReport {
            frames_written,
            underran,
        })
    }

    /// Fill `pcm` from a capture device, blocking until every frame arrives.
    ///
    /// Recovers from an overrun the same way the playback path recovers from
    /// an underrun, and reports whether it happened rather than swallowing it:
    /// a capture with a hole in it would put a step in the middle of a
    /// correlation window and read as a lag.
    pub fn read(&mut self, pcm: &mut [u8]) -> Result<WriteReport, AlsaError> {
        let lib = lib()?;
        let frame_len = self.frame_len();
        debug_assert!(frame_len > 0);

        let mut offset = 0usize;
        let mut frames_read = 0u64;
        let mut overran = false;

        while offset < pcm.len() {
            let frames_left = ((pcm.len() - offset) / frame_len) as c_ulong;
            if frames_left == 0 {
                break;
            }
            // SAFETY: the slice is live for the call and has room for exactly
            // frames_left frames of the configured format.
            let rc = unsafe {
                (lib.readi)(
                    self.handle,
                    pcm[offset..].as_mut_ptr() as *mut c_void,
                    frames_left,
                )
            };
            if rc >= 0 {
                let n = rc as usize;
                if n == 0 {
                    return Err(Pcm::error(lib, "snd_pcm_readi", &self.device, NEG_EPIPE));
                }
                offset += n * frame_len;
                frames_read += n as u64;
                continue;
            }

            let code = rc as c_int;
            if code == NEG_ENODEV {
                return Err(AlsaError::Disconnected {
                    device: self.device.clone(),
                });
            }
            if code == NEG_EPIPE {
                overran = true;
            }
            // SAFETY: handle is live.
            let recovered = unsafe { (lib.recover)(self.handle, code, 1) };
            if recovered < 0 {
                if recovered as c_int == NEG_ENODEV {
                    return Err(AlsaError::Disconnected {
                        device: self.device.clone(),
                    });
                }
                return Err(Pcm::error(lib, "snd_pcm_readi", &self.device, code));
            }
        }

        Ok(WriteReport {
            frames_written: frames_read,
            underran: overran,
        })
    }

    /// Frames the device still has to play before the next frame written
    /// becomes audible.
    ///
    /// This is `snd_pcm_delay`, which `alsa` defines as "the overall latency
    /// from the write call to the final DAC". It is not the time a write call
    /// takes to return, and it is never used to infer whether an underrun
    /// happened.
    pub fn delay_frames(&self) -> Result<i64, AlsaError> {
        let lib = lib()?;
        let mut frames: c_long = 0;
        // SAFETY: handle is live and frames is a live out-parameter.
        let rc = unsafe { (lib.delay)(self.handle, &mut frames) };
        if rc < 0 {
            let code = rc as c_int;
            if code == NEG_ENODEV {
                return Err(AlsaError::Disconnected {
                    device: self.device.clone(),
                });
            }
            if code == NEG_EPIPE {
                // Underrun. The count comes from the write path, which is the
                // one that recovers; here we simply report an empty device.
                return Ok(0);
            }
            return Err(Pcm::error(lib, "snd_pcm_delay", &self.device, code));
        }
        Ok(frames as i64)
    }

    /// Whether the device is currently in the underrun state.
    ///
    /// The device's own signal, read straight from `snd_pcm_state`.
    pub fn in_xrun(&self) -> Result<bool, AlsaError> {
        let lib = lib()?;
        // SAFETY: handle is live.
        let state = unsafe { (lib.state)(self.handle) };
        if state == STATE_DISCONNECTED {
            return Err(AlsaError::Disconnected {
                device: self.device.clone(),
            });
        }
        Ok(state == STATE_XRUN)
    }

    /// Play out everything the device already holds, then stop.
    pub fn drain(&mut self) -> Result<(), AlsaError> {
        let lib = lib()?;
        // SAFETY: handle is live.
        let rc = unsafe { (lib.drain)(self.handle) };
        if rc < 0 {
            return Err(Pcm::error(lib, "snd_pcm_drain", &self.device, rc as c_int));
        }
        Ok(())
    }

    fn close_inner(&mut self) {
        if self.closed {
            return;
        }
        self.closed = true;
        if let Ok(lib) = lib() {
            // SAFETY: handle is live and is not used again.
            unsafe {
                (lib.close)(self.handle);
            }
        }
    }
}

impl Drop for Pcm {
    fn drop(&mut self) {
        self.close_inner();
    }
}

impl fmt::Debug for Pcm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Pcm")
            .field("device", &self.device)
            .field("format", &self.format)
            .field("channels", &self.channels)
            .field("rate_hz", &self.rate_hz)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_carry_their_protocol_names_and_widths() {
        assert_eq!(Format::S16Le.bytes_per_sample(), 2);
        assert_eq!(Format::S24Packed3Le.bytes_per_sample(), 3);
        assert_eq!(Format::F32Le.bytes_per_sample(), 4);
        assert_eq!(Format::S16Le.name(), "pcm_s16le");
        assert_eq!(Format::S24Packed3Le.name(), "pcm_s24le");
        assert_eq!(Format::F32Le.name(), "pcm_f32le");
    }

    #[test]
    fn the_packed_24_bit_format_is_the_three_byte_alsa_one() {
        // ALSA's S24_LE is 24 bits in a four byte container. docs/protocol.md
        // defines pcm_s24le as three packed bytes, which is S24_3LE.
        assert_eq!(Format::S24Packed3Le.to_alsa(), 32);
        assert_ne!(Format::S24Packed3Le.to_alsa(), 6);
    }

    #[test]
    fn a_device_name_with_a_nul_is_refused_rather_than_truncated() {
        let err = Pcm::open("nu\0ll", Format::S16Le, 2, 48_000, 100_000).unwrap_err();
        match err {
            AlsaError::DeviceNameNotRepresentable { device } => assert_eq!(device, "nu\0ll"),
            // On a machine with no ALSA runtime at all the load fails first,
            // which is also a refusal rather than a truncation.
            AlsaError::RuntimeMissing { .. } => {}
            other => panic!("unexpected error {:?}", other),
        }
    }

    #[test]
    fn a_capture_device_that_is_not_there_is_a_refusal_and_never_a_silent_success() {
        // The whole point of the capture path for the measurement harness: on
        // a machine with no capture device, opening one fails and says so. It
        // never hands back a handle that would read silence.
        let err = Pcm::open_capture(
            "chorus-no-such-capture-device",
            Format::S16Le,
            2,
            96_000,
            100_000,
        )
        .unwrap_err();
        match err {
            AlsaError::Call { call, device, .. } => {
                assert_eq!(call, "snd_pcm_open");
                assert_eq!(device, "chorus-no-such-capture-device");
            }
            AlsaError::RuntimeMissing { .. } => {}
            other => panic!("unexpected error {:?}", other),
        }
    }
}
