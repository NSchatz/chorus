//! A modelled audio device, and the pieces the client tests share.
//!
//! # Why a model, and exactly what it is and is not evidence for
//!
//! The client's buffering, its behaviour at each bound, its underrun
//! accounting, its drains and its exits are all decisions the client makes
//! about what a device told it. Those decisions are testable without a device,
//! and testing them without one is the only way they get tested at all on a
//! machine with no sound card. So this file models a device: a ring that
//! drains at the nominal rate against the same monotonic clock the client
//! uses, that reports its delay the way `alsa` defines it, that blocks when it
//! is full, and that signals an underrun when it runs dry.
//!
//! **What this is evidence for**: the client's logic. Start fill, the graded
//! interval, the ceiling behaviour, the counters, the drain, the exit codes.
//!
//! **What it is not evidence for**: that `libasound` behaves as modelled, that
//! a real card's reported delay tracks this way, or that ten minutes of audio
//! came out of a speaker. Those need a device, and this repository's own
//! working agreement is blunt about the difference between a simulation and a
//! measurement. The evidence for those lives in a run on real hardware.
//!
//! This file is under `tests/`, not in the library, so no binary can reach it.

#![allow(dead_code)]

use std::io::{self, Read};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use chorus_client_linux::sink::{PcmSink, SinkError, SinkWrite};
use chorus_protocol::{encode, AudioChunk, Message, SampleFormat, StreamEnd, RESERVED_LEN};

/// A device modelled closely enough to drive the client's decisions.
pub struct ModelledDevice {
    device: String,
    rate_hz: u32,
    frame_len: usize,
    ring_frames: u64,
    /// Frames accepted and not yet played.
    queued: f64,
    /// Frames played out since the device opened.
    played: u64,
    last_tick: Instant,
    in_xrun: bool,
    /// True between a drain and the next write. A stopped device is not an
    /// underrunning one: a real `snd_pcm_drain` leaves the stream in SETUP,
    /// not in XRUN, which is exactly why draining is the right way to end a
    /// run and letting the ring dry out is not.
    stopped: bool,
    underruns: u64,
    /// Set to make every call fail, which is how a device going away mid-run
    /// is modelled.
    fails_from: Arc<Mutex<Option<Instant>>>,
}

impl ModelledDevice {
    pub fn new(device: &str, rate_hz: u32, frame_len: usize, ring_us: u64) -> ModelledDevice {
        ModelledDevice {
            device: device.to_string(),
            rate_hz,
            frame_len,
            ring_frames: ring_us * u64::from(rate_hz) / 1_000_000,
            queued: 0.0,
            played: 0,
            last_tick: Instant::now(),
            in_xrun: false,
            stopped: true,
            underruns: 0,
            fails_from: Arc::new(Mutex::new(None)),
        }
    }

    /// A handle that makes the device fail from now on, modelling a device
    /// that was removed mid-run.
    pub fn failure_switch(&self) -> Arc<Mutex<Option<Instant>>> {
        Arc::clone(&self.fails_from)
    }

    pub fn underruns(&self) -> u64 {
        self.underruns
    }

    fn failed(&self) -> bool {
        self.fails_from
            .lock()
            .map(|g| g.is_some())
            .unwrap_or(false)
    }

    /// Advance the model to now: the DAC has consumed frames at the nominal
    /// rate since the last time anyone looked.
    fn tick(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_tick).as_secs_f64();
        self.last_tick = now;
        if self.stopped {
            return;
        }
        let consumable = elapsed * f64::from(self.rate_hz);
        if consumable <= 0.0 {
            return;
        }
        if self.queued <= 0.0 {
            // Already dry, and the dry spell was counted when it started. A
            // device does not signal a new underrun every time it is asked.
            self.queued = 0.0;
            return;
        }
        let played = consumable.min(self.queued);
        self.queued -= played;
        self.played += played as u64;
        if self.queued <= 0.0 {
            // The ring ran dry while the stream was running. A real device
            // signals XRUN here, and `alsa` warns that its reported delay
            // "will not necessarily got down to 0", which is why the client
            // must never infer this from the delay.
            self.queued = 0.0;
            if !self.in_xrun {
                self.in_xrun = true;
                self.underruns += 1;
            }
        }
    }
}

impl PcmSink for ModelledDevice {
    fn device(&self) -> &str {
        &self.device
    }

    fn frame_len(&self) -> usize {
        self.frame_len
    }

    fn rate_hz(&self) -> u32 {
        self.rate_hz
    }

    fn write(&mut self, pcm: &[u8]) -> Result<SinkWrite, SinkError> {
        if self.failed() {
            return Err(SinkError::Modelled(format!(
                "modelled device '{}' was removed",
                self.device
            )));
        }
        let frames = (pcm.len() / self.frame_len) as u64;
        let mut spun = 0;
        if self.stopped {
            // A write after a drain restarts the stream, as a prepare would.
            self.stopped = false;
            self.last_tick = Instant::now();
        }
        loop {
            self.tick();
            if self.queued + frames as f64 <= self.ring_frames as f64 {
                break;
            }
            // A blocking write waits for room, exactly as ALSA's does.
            std::thread::sleep(Duration::from_millis(1));
            spun += 1;
            if spun > 60_000 {
                return Err(SinkError::Modelled(
                    "the modelled device never made room".to_string(),
                ));
            }
        }
        let underran = self.in_xrun;
        if self.in_xrun {
            // Recover, the way snd_pcm_recover does, and keep going.
            self.in_xrun = false;
        }
        self.queued += frames as f64;
        Ok(SinkWrite {
            frames_written: frames,
            underran,
        })
    }

    fn delay_frames(&mut self) -> Result<i64, SinkError> {
        if self.failed() {
            return Err(SinkError::Modelled(format!(
                "modelled device '{}' was removed",
                self.device
            )));
        }
        self.tick();
        Ok(self.queued as i64)
    }

    fn in_xrun(&mut self) -> Result<bool, SinkError> {
        if self.failed() {
            return Err(SinkError::Modelled(format!(
                "modelled device '{}' was removed",
                self.device
            )));
        }
        self.tick();
        let was = self.in_xrun;
        // The client asks once per loop; report the state once and clear it,
        // so one dry spell is one underrun rather than one per poll.
        self.in_xrun = false;
        Ok(was)
    }

    fn drain(&mut self) -> Result<(), SinkError> {
        if self.failed() {
            return Err(SinkError::Modelled(format!(
                "modelled device '{}' was removed",
                self.device
            )));
        }
        // Play out what is held, in real time, without ever running dry: a
        // drain is not an underrun. Taking real time matters, because a
        // drain that finished instantly would make the played-out frame count
        // disagree with the device's nominal rate and turn the no-rate-change
        // check into a check on the model.
        self.tick();
        let held = self.queued;
        if held > 0.0 {
            let seconds = held / f64::from(self.rate_hz);
            std::thread::sleep(Duration::from_secs_f64(seconds));
        }
        self.played += held as u64;
        self.queued = 0.0;
        self.in_xrun = false;
        self.stopped = true;
        self.last_tick = Instant::now();
        Ok(())
    }

    fn frames_played(&mut self) -> Result<u64, SinkError> {
        self.tick();
        Ok(self.played)
    }
}

/// Build one encoded audio chunk frame.
pub fn chunk_frame(sequence: u32, timestamp_ns: u64, frames: usize) -> Vec<u8> {
    encode(&Message::AudioChunk(AudioChunk {
        sequence,
        timestamp_ns,
        sample_rate_hz: 48_000,
        channels: 2,
        sample_format: SampleFormat::PcmS16Le,
        reserved: [0u8; RESERVED_LEN],
        audio_data: vec![(sequence % 251) as u8; frames * 4],
    }))
    .expect("a well formed chunk encodes")
}

/// Build the in-band end-of-stream frame.
pub fn end_frame(final_sequence: u32, end_timestamp_ns: u64) -> Vec<u8> {
    encode(&Message::StreamEnd(StreamEnd {
        final_sequence,
        end_timestamp_ns,
    }))
    .expect("a stream end encodes")
}

/// A reader that hands out bytes at a chosen pace, so a test can feed the
/// client faster or slower than its device consumes without a socket.
pub struct PacedReader {
    parts: Vec<(Duration, Vec<u8>)>,
    at: usize,
    offset: usize,
    started: Instant,
    /// Set to end the stream with a close rather than running out of parts.
    close_at_end: bool,
    reads: Arc<AtomicU64>,
}

impl PacedReader {
    pub fn new(parts: Vec<(Duration, Vec<u8>)>, close_at_end: bool) -> PacedReader {
        PacedReader {
            parts,
            at: 0,
            offset: 0,
            started: Instant::now(),
            close_at_end,
            reads: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn reads(&self) -> Arc<AtomicU64> {
        Arc::clone(&self.reads)
    }
}

impl Read for PacedReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        loop {
            if self.at >= self.parts.len() {
                if self.close_at_end {
                    return Ok(0);
                }
                std::thread::sleep(Duration::from_millis(5));
                return Err(io::Error::new(io::ErrorKind::WouldBlock, "paced reader idle"));
            }
            let (due, bytes) = &self.parts[self.at];
            if self.started.elapsed() < *due {
                std::thread::sleep(Duration::from_millis(1));
                return Err(io::Error::new(io::ErrorKind::WouldBlock, "not due yet"));
            }
            let remaining = &bytes[self.offset..];
            if remaining.is_empty() {
                self.at += 1;
                self.offset = 0;
                continue;
            }
            let n = remaining.len().min(buf.len());
            buf[..n].copy_from_slice(&remaining[..n]);
            self.offset += n;
            self.reads.fetch_add(1, Ordering::Relaxed);
            return Ok(n);
        }
    }
}

/// A stream of `count` chunks, each due at its nominal time scaled by
/// `rate_scale` (below 1.0 is faster than real time).
pub fn paced_chunks(
    count: u32,
    frames_per_chunk: usize,
    chunk_us: u64,
    rate_scale: f64,
    close_at_end: bool,
    with_end_signal: bool,
) -> PacedReader {
    let mut parts = Vec::new();
    for i in 0..count {
        let due = Duration::from_micros((chunk_us as f64 * i as f64 * rate_scale) as u64);
        parts.push((
            due,
            chunk_frame(
                i,
                u64::from(i) * chunk_us * 1_000,
                frames_per_chunk,
            ),
        ));
    }
    if with_end_signal {
        let due = Duration::from_micros((chunk_us as f64 * count as f64 * rate_scale) as u64);
        parts.push((
            due,
            end_frame(count.saturating_sub(1), u64::from(count) * chunk_us * 1_000),
        ));
    }
    PacedReader::new(parts, close_at_end)
}
