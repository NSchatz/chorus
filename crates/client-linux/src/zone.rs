//! Zone volume and mute, applied to the PCM on its way to the sink.
//!
//! # Where this runs, and why it is here rather than anywhere else
//!
//! At the very last point before [`crate::sink::PcmSink::write`]. Everything
//! upstream of it - the buffer, the sync loop, the playout corrector - is about
//! WHEN a sample becomes audible, and this is the only thing in the client that
//! changes WHAT the sample is. Putting it last means the sync loop's frame
//! accounting is untouched by it: a muted zone writes exactly as many frames as
//! an unmuted one, at exactly the same instants, and the device's reported
//! delay is what it would have been. A mute that stopped writing would be a
//! mute that silently changed the endpoint's alignment, and coming back from it
//! would be a resync.
//!
//! # It is graded on the samples, never on a status line
//!
//! `crates/client-linux/tests/zone_apply.rs` reads what a modelled
//! [`crate::sink::PcmSink`] ACCEPTED and asserts the scaling on those bytes.
//! That is the criterion's own wording, and it is why this is a pure function
//! over a buffer with no reporting in it at all: there is nothing here that
//! could say it had applied a volume it had not.
//!
//! # The factor is exact, and the error is one unit in the last place
//!
//! The catalog carries a volume as thousandths of full scale
//! (`docs/decisions/0016-the-control-catalog.md`), so scaling an integer sample
//! is an integer multiply and an integer divide, with no floating point on the
//! path at all for the two integer formats. Truncation toward zero is the one
//! rounding this does, so a scaled sample is within one unit in the last place
//! of the exact product, in the direction of silence. `pcm_f32le` is scaled in
//! `f32` because its samples already are.
//!
//! # The room's volume from the audio wire (goal 11)
//!
//! [`RoomGain`] is the same enforcement the C endpoint makes
//! (`firmware/include/chorus/volume.h`, ADR 0074): the server's `room_volume`
//! (`docs/protocol.md`, "0x38 room volume") gives a gain, the room's limit and
//! a ramp, and what is written at every frame is the least of the ramped gain,
//! that limit, this endpoint's own ceiling (`--max-volume`) and the gain the
//! control plane's state gives ([`crate::control::ZoneWatch::gain`]): the most
//! restrictive bound wins, whichever path it came by, so neither a server bug
//! nor a hostile control path takes this endpoint above its room's limit or
//! its own ceiling (K81, I10, brief section 4.8).
//!
//! When no ramp is moving the applied gain is a whole number of thousandths
//! and is applied by [`ZoneGain`] exactly as before. While a ramp moves, the
//! gain is a Q16 fraction per frame, ramped linear in amplitude by the C
//! endpoint's arithmetic (from + (to - from) * k / N, truncated), counted in
//! frames written at the stream's rate, which is monotonic by construction.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use chorus_control::catalog::{Volume, VOLUME_SCALE};
use chorus_protocol::v2::{RoomVolume, Sound, MAX_ROOM_VOLUME_RAMP_MS};
use chorus_protocol::SampleFormat;

/// How far a scaled sample may be from the exact product, in units of the last
/// place.
///
/// One, and in the direction of silence, because the only rounding here is the
/// truncation of an integer divide. The test asserts this bound rather than
/// asserting equality, and this constant is what it asserts.
pub const SCALING_TOLERANCE_ULP: i64 = 1;

/// Applies a zone's gain to interleaved PCM in the format the stream announced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ZoneGain {
    format: SampleFormat,
}

impl ZoneGain {
    /// A gain applier for one stream's format.
    pub fn new(format: SampleFormat) -> ZoneGain {
        ZoneGain { format }
    }

    /// Scale `pcm` in place by `gain`.
    ///
    /// Full scale is the identity and returns without touching a byte, which is
    /// the ordinary case and the one that has to cost nothing. Silence writes
    /// zeros rather than multiplying by zero, which is the same answer and is
    /// exact for every format including the float one.
    pub fn apply(&self, gain: Volume, pcm: &mut [u8]) {
        if gain == Volume::FULL {
            return;
        }
        if gain == Volume::SILENT {
            // A muted zone hands the device the same number of frames it would
            // otherwise have, and every sample in them is zero. Frames keep
            // flowing; only their content changes.
            pcm.fill(0);
            return;
        }
        let numerator = i64::from(gain.thousandths());
        let denominator = i64::from(VOLUME_SCALE);
        match self.format {
            SampleFormat::PcmS16Le => {
                for sample in pcm.as_chunks_mut::<2>().0 {
                    let value = i64::from(i16::from_le_bytes([sample[0], sample[1]]));
                    let scaled = (value * numerator / denominator) as i16;
                    sample.copy_from_slice(&scaled.to_le_bytes());
                }
            }
            SampleFormat::PcmS24Le => {
                for sample in pcm.as_chunks_mut::<3>().0 {
                    let value = sign_extend_24(sample);
                    let scaled = value * numerator / denominator;
                    let bytes = (scaled as i32).to_le_bytes();
                    sample.copy_from_slice(&bytes[..3]);
                }
            }
            SampleFormat::PcmF32Le => {
                let factor = gain.thousandths() as f32 / VOLUME_SCALE as f32;
                for sample in pcm.as_chunks_mut::<4>().0 {
                    let value = f32::from_le_bytes([sample[0], sample[1], sample[2], sample[3]]);
                    sample.copy_from_slice(&(value * factor).to_le_bytes());
                }
            }
        }
    }
}

impl ZoneGain {
    /// The format this applier scales.
    pub fn format(&self) -> SampleFormat {
        self.format
    }
}

/// Full amplitude as a Q16 fraction: the unit a ramping gain is held in.
pub const UNITY_Q16: u32 = 1 << 16;

/// A thousandths value as Q16, rounded down (toward silence); exact at 0 and
/// at 1000, the C endpoint's `chorus_volume_q16_from_thousandths`.
pub fn q16_from_thousandths(thousandths: u32) -> u32 {
    if thousandths >= VOLUME_SCALE {
        return UNITY_Q16;
    }
    (u64::from(thousandths) * u64::from(UNITY_Q16) / u64::from(VOLUME_SCALE)) as u32
}

/// Where the session's `room_volume` messages wait for the playout loop.
///
/// The session's reader delivers into it on the receiving thread; the
/// playout loop takes the latest once per chunk. Only the latest matters: two
/// messages inside one chunk's time leave the endpoint where the second one
/// says, which is where it would have been a chunk later anyway.
#[derive(Debug, Default)]
pub struct RoomVolumeInbox {
    latest: Mutex<Option<RoomVolume>>,
    received: AtomicU64,
}

impl RoomVolumeInbox {
    /// A `room_volume` from the session, already validated by the decoder.
    pub fn deliver(&self, message: RoomVolume) {
        let mut slot = match self.latest.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        *slot = Some(message);
        self.received.fetch_add(1, Ordering::Relaxed);
    }

    /// The latest message not yet taken, if any.
    pub fn take(&self) -> Option<RoomVolume> {
        match self.latest.lock() {
            Ok(mut g) => g.take(),
            Err(poisoned) => poisoned.into_inner().take(),
        }
    }

    /// How many messages have been delivered.
    pub fn received(&self) -> u64 {
        self.received.load(Ordering::Relaxed)
    }
}

/// Where the session's `sound` messages (0x39, goal 12) wait, and the last
/// one received.
///
/// Phase A of goal 12 only keeps it: the playout loop takes each new one to
/// log it, and [`SoundInbox::last`] is what the endpoint DSP configures its
/// chain from (the chain itself is the endpoint DSP track's). Held for the
/// life of the process, as the room's gain is: a new session is not a reason
/// to forget how the room sounds.
#[derive(Debug, Default)]
pub struct SoundInbox {
    fresh: Mutex<Option<Sound>>,
    last: Mutex<Option<Sound>>,
    received: AtomicU64,
}

impl SoundInbox {
    /// A `sound` from the session, already validated by the decoder.
    pub fn deliver(&self, message: Sound) {
        *lock(&self.last) = Some(message.clone());
        *lock(&self.fresh) = Some(message);
        self.received.fetch_add(1, Ordering::Relaxed);
    }

    /// The latest message not yet taken, if any.
    pub fn take(&self) -> Option<Sound> {
        lock(&self.fresh).take()
    }

    /// The last message received, taken or not.
    pub fn last(&self) -> Option<Sound> {
        lock(&self.last).clone()
    }

    /// How many messages have been delivered.
    pub fn received(&self) -> u64 {
        self.received.load(Ordering::Relaxed)
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    match m.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    }
}

/// One line for a `sound`: its fields, as the client's delay log and the
/// tests spell them.
pub fn sound_line(m: &Sound) -> String {
    format!(
        "bass_db={} treble_db={} flags=0x{:02x} role={} sub_present={} crossover_hz={} \
         sub_level_cdb={} eq_count={}{}",
        m.bass_db,
        m.treble_db,
        m.flags,
        m.role,
        u8::from(m.sub_present),
        m.crossover_hz,
        m.sub_level_cdb,
        m.filters.len(),
        m.filters
            .iter()
            .map(|f| format!(" eq={}/{}/{}", f.freq_hz, f.gain_cdb, f.q_milli))
            .collect::<String>()
    )
}

/// The room's gain, limit and ramp, and this endpoint's ceiling.
#[derive(Debug, Clone)]
pub struct RoomGain {
    ceiling: Volume,
    limit: Volume,
    /// The gain the last message asked for; the ceiling until the first one.
    gain: Volume,
    from_q16: u32,
    to_q16: u32,
    ramp_q16: u32,
    ramp_frames: u64,
    done_frames: u64,
    step_q16: u32,
    step_rem: u64,
    acc: u64,
    messages: u64,
}

impl RoomGain {
    /// Start at the ceiling with no limit received: the startup gain is the
    /// ceiling (ADR 0074), so an endpoint whose server never sends
    /// `room_volume` plays as it did before goal 11.
    pub fn new(ceiling: Volume) -> RoomGain {
        let q = q16_from_thousandths(ceiling.thousandths());
        RoomGain {
            ceiling,
            limit: Volume::FULL,
            gain: ceiling,
            from_q16: q,
            to_q16: q,
            ramp_q16: q,
            ramp_frames: 0,
            done_frames: 0,
            step_q16: 0,
            step_rem: 0,
            acc: 0,
            messages: 0,
        }
    }

    /// This endpoint's own ceiling.
    pub fn ceiling(&self) -> Volume {
        self.ceiling
    }

    /// The last limit received; full scale before the first.
    pub fn limit(&self) -> Volume {
        self.limit
    }

    /// `room_volume` messages taken.
    pub fn messages(&self) -> u64 {
        self.messages
    }

    /// Whether a ramp is still moving.
    pub fn ramping(&self) -> bool {
        self.done_frames < self.ramp_frames
    }

    /// What the next frame plays at, Q16, under the control plane's `zone`
    /// gain: min(ramped gain, limit, ceiling, zone).
    pub fn applied_q16(&self, zone: Volume) -> u32 {
        let ramped = if self.ramping() {
            self.ramp_q16
        } else {
            q16_from_thousandths(self.gain.thousandths())
        };
        ramped
            .min(q16_from_thousandths(self.limit.thousandths()))
            .min(q16_from_thousandths(self.ceiling.thousandths()))
            .min(q16_from_thousandths(zone.thousandths()))
    }

    /// The applied gain when no ramp is moving, in whole thousandths.
    pub fn settled(&self, zone: Volume) -> Volume {
        let t = self
            .gain
            .thousandths()
            .min(self.limit.thousandths())
            .min(self.ceiling.thousandths())
            .min(zone.thousandths());
        Volume::from_thousandths(i64::from(t)).unwrap_or(Volume::SILENT)
    }

    /// Take a `room_volume`. The ramp starts from what is being applied now
    /// (under `zone` too), so what is heard never jumps; the limit applies
    /// from the next frame. A value past its wire range (which the decoder
    /// never passes) is taken at the range's end.
    pub fn set(&mut self, message: &RoomVolume, zone: Volume, rate_hz: u32) {
        let from = self.applied_q16(zone);
        let clamp =
            |v: u16| Volume::from_thousandths(i64::from(v.min(1000))).unwrap_or(Volume::FULL);
        self.gain = clamp(message.gain);
        self.limit = clamp(message.limit);
        let ramp_ms = u64::from(message.ramp_ms.min(MAX_ROOM_VOLUME_RAMP_MS));
        let to = q16_from_thousandths(self.gain.thousandths());
        self.from_q16 = from;
        self.to_q16 = to;
        self.ramp_frames = ramp_ms * u64::from(rate_hz) / 1000;
        self.done_frames = 0;
        self.acc = 0;
        let span = u64::from(to.abs_diff(from));
        if self.ramp_frames == 0 || span == 0 {
            self.ramp_frames = 0;
            self.ramp_q16 = to;
            self.step_q16 = 0;
            self.step_rem = 0;
        } else {
            self.ramp_q16 = from;
            self.step_q16 = (span / self.ramp_frames) as u32;
            self.step_rem = span % self.ramp_frames;
        }
        self.messages += 1;
    }

    /// One frame of the ramp, by the remainder accumulator, ending exactly on
    /// the target.
    fn step(&mut self) {
        if !self.ramping() {
            return;
        }
        self.done_frames += 1;
        let mut delta = self.step_q16;
        self.acc += self.step_rem;
        if self.acc >= self.ramp_frames {
            self.acc -= self.ramp_frames;
            delta += 1;
        }
        self.ramp_q16 = if self.to_q16 > self.from_q16 {
            self.ramp_q16 + delta
        } else {
            self.ramp_q16 - delta
        };
        if self.done_frames == self.ramp_frames {
            self.ramp_q16 = self.to_q16;
        }
    }

    /// Scale interleaved `pcm` of `channels` channels in place, frame by
    /// frame, and advance the ramp by the frames it held. The number of frames
    /// never changes; a settled gain goes through [`ZoneGain`] unchanged.
    pub fn apply(&mut self, zone: Volume, applier: &ZoneGain, channels: usize, pcm: &mut [u8]) {
        let format = applier.format();
        let frame_len = channels.max(1) * format.bytes_per_sample();
        let mut at = 0;
        while self.ramping() && at + frame_len <= pcm.len() {
            let g = self.applied_q16(zone);
            if g < UNITY_Q16 {
                scale_frame_q16(format, g, &mut pcm[at..at + frame_len]);
            }
            self.step();
            at += frame_len;
        }
        if at < pcm.len() {
            applier.apply(self.settled(zone), &mut pcm[at..]);
        }
    }
}

/// One frame's samples scaled by a Q16 gain, truncated toward zero.
fn scale_frame_q16(format: SampleFormat, g: u32, frame: &mut [u8]) {
    let g = i64::from(g);
    let unity = i64::from(UNITY_Q16);
    match format {
        SampleFormat::PcmS16Le => {
            for sample in frame.as_chunks_mut::<2>().0 {
                let value = i64::from(i16::from_le_bytes([sample[0], sample[1]]));
                sample.copy_from_slice(&((value * g / unity) as i16).to_le_bytes());
            }
        }
        SampleFormat::PcmS24Le => {
            for sample in frame.as_chunks_mut::<3>().0 {
                let scaled = sign_extend_24(sample) * g / unity;
                sample.copy_from_slice(&(scaled as i32).to_le_bytes()[..3]);
            }
        }
        SampleFormat::PcmF32Le => {
            let factor = g as f32 / unity as f32;
            for sample in frame.as_chunks_mut::<4>().0 {
                let value = f32::from_le_bytes([sample[0], sample[1], sample[2], sample[3]]);
                sample.copy_from_slice(&(value * factor).to_le_bytes());
            }
        }
    }
}

/// A packed 24-bit little-endian sample as the number it is.
fn sign_extend_24(bytes: &[u8]) -> i64 {
    let raw = u32::from(bytes[0]) | (u32::from(bytes[1]) << 8) | (u32::from(bytes[2]) << 16);
    if raw & 0x0080_0000 != 0 {
        i64::from(raw as i32 | !0x00FF_FFFF)
    } else {
        i64::from(raw as i32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sound_is_taken_once_for_the_log_and_kept_as_the_last() {
        use chorus_protocol::v2::SoundFilter;
        let inbox = SoundInbox::default();
        assert_eq!((inbox.take(), inbox.last()), (None, None));
        let mut sound = Sound {
            bass_db: 3,
            treble_db: -2,
            flags: 0x19,
            role: 4,
            sub_present: true,
            crossover_hz: 100,
            sub_level_cdb: -350,
            filters: vec![SoundFilter {
                freq_hz: 42,
                gain_cdb: -600,
                q_milli: 4500,
            }],
        };
        inbox.deliver(sound.clone());
        assert_eq!(inbox.take(), Some(sound.clone()));
        assert_eq!(inbox.take(), None, "taken once");
        assert_eq!(inbox.last(), Some(sound.clone()), "and kept");
        assert_eq!(
            sound_line(&sound),
            "bass_db=3 treble_db=-2 flags=0x19 role=4 sub_present=1 crossover_hz=100 \
             sub_level_cdb=-350 eq_count=1 eq=42/-600/4500"
        );
        sound.bass_db = 0;
        inbox.deliver(sound.clone());
        assert_eq!((inbox.last(), inbox.received()), (Some(sound), 2));
    }

    fn s16(samples: &[i16]) -> Vec<u8> {
        samples.iter().flat_map(|s| s.to_le_bytes()).collect()
    }

    fn read_s16(pcm: &[u8]) -> Vec<i16> {
        pcm.as_chunks::<2>()
            .0
            .iter()
            .map(|c| i16::from_le_bytes(*c))
            .collect()
    }

    #[test]
    fn full_scale_changes_nothing_at_all() {
        let original = s16(&[-32_768, -1, 0, 1, 32_767]);
        let mut pcm = original.clone();
        ZoneGain::new(SampleFormat::PcmS16Le).apply(Volume::FULL, &mut pcm);
        assert_eq!(pcm, original);
    }

    #[test]
    fn silence_is_every_sample_zero_and_the_same_number_of_frames() {
        let mut pcm = s16(&[-32_768, -1, 3_000, 32_767]);
        let frames = pcm.len();
        ZoneGain::new(SampleFormat::PcmS16Le).apply(Volume::SILENT, &mut pcm);
        assert_eq!(pcm.len(), frames, "a mute must not shorten a chunk");
        assert!(pcm.iter().all(|b| *b == 0));
    }

    #[test]
    fn a_scaled_sample_is_within_one_unit_of_the_exact_product() {
        let gain = Volume::from_thousandths(375).unwrap();
        let samples: Vec<i16> = (-32_768..32_767).step_by(7).collect();
        let mut pcm = s16(&samples);
        ZoneGain::new(SampleFormat::PcmS16Le).apply(gain, &mut pcm);
        for (before, after) in samples.iter().zip(read_s16(&pcm)) {
            let exact = f64::from(*before) * 0.375;
            let error = f64::from(after) - exact;
            assert!(
                error.abs() <= SCALING_TOLERANCE_ULP as f64,
                "{} scaled to {} against an exact {}",
                before,
                after,
                exact
            );
        }
    }

    #[test]
    fn the_twenty_four_bit_format_keeps_its_sign() {
        let gain = Volume::from_thousandths(500).unwrap();
        // -8388608, -1, 1, 8388607, packed little-endian in three bytes each.
        let mut pcm = vec![
            0x00, 0x00, 0x80, 0xFF, 0xFF, 0xFF, 0x01, 0x00, 0x00, 0xFF, 0xFF, 0x7F,
        ];
        ZoneGain::new(SampleFormat::PcmS24Le).apply(gain, &mut pcm);
        let values: Vec<i64> = pcm
            .as_chunks::<3>()
            .0
            .iter()
            .map(|c| sign_extend_24(c))
            .collect();
        assert_eq!(values, vec![-4_194_304, 0, 0, 4_194_303]);
    }

    #[test]
    fn the_float_format_is_scaled_as_a_float() {
        let gain = Volume::from_thousandths(250).unwrap();
        let mut pcm: Vec<u8> = [-1.0f32, -0.5, 0.0, 0.5, 1.0]
            .iter()
            .flat_map(|s| s.to_le_bytes())
            .collect();
        ZoneGain::new(SampleFormat::PcmF32Le).apply(gain, &mut pcm);
        let values: Vec<f32> = pcm
            .as_chunks::<4>()
            .0
            .iter()
            .map(|c| f32::from_le_bytes(*c))
            .collect();
        assert_eq!(values, vec![-0.25, -0.125, 0.0, 0.125, 0.25]);
    }

    #[test]
    fn scaling_never_wraps_at_either_extreme() {
        // The one arithmetic hazard here: a multiply that overflowed would turn
        // the loudest possible sample into the quietest, which is a click at
        // full scale and would be inaudible in a test that only checked the
        // middle of the range.
        for thousandths in [0i64, 1, 499, 500, 501, 999, 1_000] {
            let gain = Volume::from_thousandths(thousandths).unwrap();
            let mut pcm = s16(&[i16::MIN, i16::MAX]);
            ZoneGain::new(SampleFormat::PcmS16Le).apply(gain, &mut pcm);
            let values = read_s16(&pcm);
            assert!(
                values[0] <= 0,
                "{} turned i16::MIN into {}",
                thousandths,
                values[0]
            );
            assert!(
                values[1] >= 0,
                "{} turned i16::MAX into {}",
                thousandths,
                values[1]
            );
        }
    }

    fn rv(gain: u16, limit: u16, ramp_ms: u16) -> RoomVolume {
        RoomVolume {
            gain,
            limit,
            ramp_ms,
        }
    }

    fn v(t: u32) -> Volume {
        Volume::from_thousandths(i64::from(t)).unwrap()
    }

    #[test]
    fn q16_is_exact_at_both_ends_and_monotone() {
        assert_eq!(q16_from_thousandths(0), 0);
        assert_eq!(q16_from_thousandths(1000), UNITY_Q16);
        assert!((1..=1000).all(|t| q16_from_thousandths(t) >= q16_from_thousandths(t - 1)));
    }

    #[test]
    fn the_applied_gain_is_the_least_of_gain_limit_ceiling_and_zone() {
        let mut r = RoomGain::new(Volume::FULL);
        assert_eq!(
            r.settled(Volume::FULL),
            Volume::FULL,
            "startup: the ceiling"
        );
        r.set(&rv(900, 600, 0), Volume::FULL, 48_000);
        assert_eq!(r.settled(Volume::FULL), v(600), "a gain above the limit");
        assert_eq!(r.settled(v(250)), v(250), "the zone gain lower still");
        let mut r = RoomGain::new(v(300));
        r.set(&rv(1000, 1000, 0), Volume::FULL, 48_000);
        assert_eq!(r.settled(Volume::FULL), v(300), "the ceiling");
        r.set(&rv(1000, 100, 0), Volume::FULL, 48_000);
        assert_eq!(r.settled(Volume::FULL), v(100), "a limit below the ceiling");
    }

    #[test]
    fn a_ramp_matches_the_closed_form_at_every_frame_and_ends_on_its_target() {
        let mut r = RoomGain::new(Volume::FULL);
        r.set(&rv(200, 1000, 10), Volume::FULL, 48_000);
        let n = 480u64;
        let from = UNITY_Q16 as u64;
        let to = q16_from_thousandths(200) as u64;
        let format = ZoneGain::new(SampleFormat::PcmS16Le);
        let mut prev = u32::MAX;
        for k in 0..=n + 3 {
            let expected = if k >= n {
                to
            } else {
                from - (from - to) * k / n
            };
            let g = r.applied_q16(Volume::FULL);
            assert_eq!(u64::from(g), expected, "frame {}", k);
            assert!(g <= prev, "non-increasing at frame {}", k);
            prev = g;
            let mut frame = s16(&[1000, -1000]);
            r.apply(Volume::FULL, &format, 2, &mut frame);
            assert_eq!(frame.len(), 4, "a frame stays a frame");
        }
        assert!(!r.ramping());
        assert_eq!(r.settled(Volume::FULL), v(200));
    }

    #[test]
    fn a_new_ramp_starts_from_what_is_heard_and_frames_are_never_added_or_lost() {
        let mut r = RoomGain::new(Volume::FULL);
        r.set(&rv(800, 400, 0), Volume::FULL, 48_000);
        r.set(&rv(0, 400, 100), Volume::FULL, 48_000);
        assert_eq!(
            r.applied_q16(Volume::FULL),
            q16_from_thousandths(400),
            "from the limit being heard, not the gain held above it"
        );
        let mut pcm = s16(&[12_000; 960 * 2]);
        let len = pcm.len();
        r.apply(
            Volume::FULL,
            &ZoneGain::new(SampleFormat::PcmS16Le),
            2,
            &mut pcm,
        );
        assert_eq!(pcm.len(), len);
        let left: Vec<i16> = read_s16(&pcm).into_iter().step_by(2).collect();
        assert_eq!(
            left[0],
            (12_000i64 * i64::from(q16_from_thousandths(400)) / 65_536) as i16
        );
        assert!(left.windows(2).all(|w| w[1] <= w[0]));
        assert!(r.ramping(), "960 frames into a 4800-frame ramp");
    }
}
