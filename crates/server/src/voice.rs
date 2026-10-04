//! The voice path's intake: what the server does with a speaker's microphone
//! audio (proposal P8, Option A; K73, I4; `docs/decisions/0166-*` for the
//! wire, `docs/decisions/0169-*` for the intake).
//!
//! # The rule
//!
//! A `mic_audio` frame is kept only when all of these hold at the moment it
//! arrives, and is dropped and counted otherwise ([`DropReason`]):
//!
//! 1. its session declared the `voice` role;
//! 2. its endpoint is in a room;
//! 3. that room has voice switched on (`voice_enabled`, off by default);
//! 4. the session's own last `mic_state` said the gate is `live` (a session
//!    that has said nothing is muted).
//!
//! The first three are read from the room model by the caller for every
//! frame, so a room switched off drops its next frame, whether or not the
//! endpoint has yet obeyed the `voice_control` that tells it to stop. The
//! fourth is the endpoint's own word about its hardware switch: nothing here
//! or anywhere in the server opens it.
//!
//! # Where the audio is, and where it is not (I4)
//!
//! A kept frame's samples go into this session's [`Voice`] buffer and nowhere
//! else: a ring of at most [`BUFFER_SAMPLES`] samples in memory, the oldest
//! falling off the end. The buffer is wiped when the room's voice is switched
//! off, when the gate closes and when the session ends, so a muted or
//! disabled room holds no audio at all. Nothing in this module writes a file,
//! opens a socket, or hands samples to the router, a slot, a line-in port or
//! the visualizer; it has no reference to any of them. The only readers are
//! [`Voice::buffered`] and [`Voice::take`], for the wake word (a later task).
//! A status line names counts and never a sample.
//!
//! Control code: it stamps nothing and reads no clock. `audio-path.conf`
//! records it as excluded.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard};

use chorus_protocol::v2::{MicAudio, VoiceControl, MIC_BYTES_PER_SAMPLE, MIC_SAMPLE_RATE_HZ};

/// The most samples one session's buffer holds: three seconds at the wire's
/// 16 kHz. ASSUMED: enough for a wake-word model's window with room to spare,
/// not measured; the wake-word task settles it.
pub const BUFFER_SAMPLES: usize = MIC_SAMPLE_RATE_HZ as usize * 3;

/// The most bytes one session's buffer holds.
pub const BUFFER_BYTES: usize = BUFFER_SAMPLES * MIC_BYTES_PER_SAMPLE;

/// Why a `mic_audio` frame was not kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropReason {
    /// The session's `hello` did not declare the voice role.
    NoVoiceRole,
    /// The endpoint is in no room.
    NoRoom,
    /// The endpoint's room has voice switched off.
    VoiceDisabled,
    /// The session's last `mic_state` said muted, or it has sent none.
    GateMuted,
}

impl DropReason {
    /// Every reason, in the order the counters are kept.
    pub const ALL: [DropReason; 4] = [
        DropReason::NoVoiceRole,
        DropReason::NoRoom,
        DropReason::VoiceDisabled,
        DropReason::GateMuted,
    ];

    /// The reason as a status line names it.
    pub fn name(self) -> &'static str {
        match self {
            DropReason::NoVoiceRole => "no-voice-role",
            DropReason::NoRoom => "no-room",
            DropReason::VoiceDisabled => "voice-disabled",
            DropReason::GateMuted => "gate-muted",
        }
    }

    fn index(self) -> usize {
        match self {
            DropReason::NoVoiceRole => 0,
            DropReason::NoRoom => 1,
            DropReason::VoiceDisabled => 2,
            DropReason::GateMuted => 3,
        }
    }
}

/// What became of one `mic_audio` frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Intake {
    /// Its samples are in the session's buffer.
    Buffered,
    /// It was dropped, and counted under this reason.
    Dropped(DropReason),
}

/// The room a frame's endpoint is in, as the caller read it off the room
/// model when the frame arrived.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RoomVoice<'a> {
    /// The room's id.
    pub room: &'a str,
    /// Whether the room has voice switched on.
    pub enabled: bool,
}

struct Entry {
    session: u64,
    endpoint: String,
    voice_role: bool,
    gate_live: bool,
    told: VoiceControl,
    buffer: VecDeque<u8>,
    last: Option<Intake>,
    buffered_frames: u64,
    dropped_frames: u64,
}

impl Entry {
    /// Overwrite what the buffer holds, then empty it: a muted or disabled
    /// room keeps no audio.
    fn wipe(&mut self) {
        for byte in self.buffer.iter_mut() {
            *byte = 0;
        }
        self.buffer.clear();
    }

    fn counts(&self) -> String {
        format!(
            "buffered_frames={} dropped_frames={}",
            self.buffered_frames, self.dropped_frames
        )
    }
}

/// Every session's microphone state and buffer, and the drop counters.
#[derive(Default)]
pub struct Voice {
    entries: Mutex<Vec<Entry>>,
    dropped: [AtomicU64; 4],
    buffered: AtomicU64,
}

impl std::fmt::Debug for Voice {
    // Counts only: a buffer is never printed.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Voice")
            .field("sessions", &self.locked().len())
            .field("buffered_frames", &self.buffered_frames())
            .finish_non_exhaustive()
    }
}

const OFF: VoiceControl = VoiceControl {
    uplink: false,
    listening: false,
};

impl Voice {
    /// No session, nothing buffered, nothing dropped.
    pub fn new() -> Voice {
        Voice::default()
    }

    fn locked(&self) -> MutexGuard<'_, Vec<Entry>> {
        match self.entries.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    /// A session is up. Its gate is muted and its uplink off until it and
    /// the server say otherwise (`docs/protocol.md`, "The voice role").
    pub fn session_up(&self, session: u64, endpoint: &str, voice_role: bool) {
        let mut entries = self.locked();
        entries.retain(|e| e.session != session);
        entries.push(Entry {
            session,
            endpoint: endpoint.to_string(),
            voice_role,
            gate_live: false,
            told: OFF,
            buffer: VecDeque::new(),
            last: None,
            buffered_frames: 0,
            dropped_frames: 0,
        });
    }

    /// A session has ended: its buffer is wiped and it is forgotten. Returns
    /// whether another session of the same endpoint still reports a live
    /// gate (what the room model is then told of the endpoint), and the
    /// status line when the session had sent any microphone audio.
    pub fn session_down(&self, session: u64) -> (bool, Option<String>) {
        let mut entries = self.locked();
        let Some(at) = entries.iter().position(|e| e.session == session) else {
            return (false, None);
        };
        let mut entry = entries.remove(at);
        entry.wipe();
        let still_live = entries
            .iter()
            .any(|e| e.endpoint == entry.endpoint && e.gate_live);
        let line = (entry.buffered_frames + entry.dropped_frames > 0)
            .then(|| format!("voice mic id={} ended {}", entry.endpoint, entry.counts()));
        (still_live, line)
    }

    /// The session's endpoint said its gate is `live` or muted (`mic_state`).
    /// Closing it wipes the buffer. `None` for a session that did not declare
    /// the voice role (its word is not taken) or is not known; else whether
    /// the gate changed, and the status line when it did.
    pub fn gate(&self, session: u64, live: bool) -> Option<(bool, Option<String>)> {
        let mut entries = self.locked();
        let entry = entries
            .iter_mut()
            .find(|e| e.session == session && e.voice_role)?;
        if entry.gate_live == live {
            return Some((false, None));
        }
        entry.gate_live = live;
        if !live {
            entry.wipe();
        }
        Some((
            true,
            Some(format!(
                "voice mic id={} gate={} {}",
                entry.endpoint,
                if live { "live" } else { "muted" },
                entry.counts()
            )),
        ))
    }

    /// One `mic_audio` frame from `session`, whose endpoint is in `room`
    /// (`None`: in no room) as the room model says now. Returns what became
    /// of it, and a status line when that differs from what became of the
    /// session's frame before (so a stream of frames is one line, not one a
    /// frame). The line carries counts, never samples.
    pub fn audio(
        &self,
        session: u64,
        room: Option<RoomVoice<'_>>,
        frame: &MicAudio,
    ) -> (Intake, Option<String>) {
        let mut entries = self.locked();
        let Some(entry) = entries.iter_mut().find(|e| e.session == session) else {
            // A session this module was never told of declared nothing.
            self.dropped[DropReason::NoVoiceRole.index()].fetch_add(1, Ordering::Relaxed);
            return (Intake::Dropped(DropReason::NoVoiceRole), None);
        };
        let intake = match room {
            _ if !entry.voice_role => Intake::Dropped(DropReason::NoVoiceRole),
            None => Intake::Dropped(DropReason::NoRoom),
            Some(r) if !r.enabled => Intake::Dropped(DropReason::VoiceDisabled),
            Some(_) if !entry.gate_live => Intake::Dropped(DropReason::GateMuted),
            Some(_) => Intake::Buffered,
        };
        match intake {
            Intake::Buffered => {
                // The decoder holds a frame to whole samples and to at most
                // MIC_MAX_SAMPLES of them, far below the bound.
                let data = &frame.data[..frame.data.len().min(BUFFER_BYTES)];
                let over = (entry.buffer.len() + data.len()).saturating_sub(BUFFER_BYTES);
                entry.buffer.drain(..over);
                entry.buffer.extend(data.iter().copied());
                entry.buffered_frames += 1;
                self.buffered.fetch_add(1, Ordering::Relaxed);
            }
            Intake::Dropped(reason) => {
                // What was kept while the room was listening does not
                // outlast it.
                entry.wipe();
                entry.dropped_frames += 1;
                self.dropped[reason.index()].fetch_add(1, Ordering::Relaxed);
            }
        }
        let line = (entry.last != Some(intake)).then(|| {
            let room = room.map_or("-", |r| r.room);
            match intake {
                Intake::Buffered => format!(
                    "voice mic id={} room={} intake=buffering {}",
                    entry.endpoint,
                    room,
                    entry.counts()
                ),
                Intake::Dropped(reason) => format!(
                    "voice mic id={} room={} intake=dropping reason={} {}",
                    entry.endpoint,
                    room,
                    reason.name(),
                    entry.counts()
                ),
            }
        });
        entry.last = Some(intake);
        (intake, line)
    }

    /// What `session` was last told (`voice_control`); off and not listening
    /// until told otherwise. `None` for a session not known here.
    pub fn told(&self, session: u64) -> Option<VoiceControl> {
        self.locked()
            .iter()
            .find(|e| e.session == session)
            .map(|e| e.told)
    }

    /// The server has sent `session` this `voice_control`. Turning the uplink
    /// off wipes the buffer. Returns the status line, or `None` for a session
    /// not known here.
    pub fn tell(&self, session: u64, control: VoiceControl) -> Option<String> {
        let mut entries = self.locked();
        let entry = entries.iter_mut().find(|e| e.session == session)?;
        entry.told = control;
        if !control.uplink {
            entry.wipe();
        }
        Some(format!(
            "voice control id={} uplink={} listening={} {}",
            entry.endpoint,
            u8::from(control.uplink),
            u8::from(control.listening),
            entry.counts()
        ))
    }

    /// A copy of what `endpoint`'s buffers hold, oldest first: whole 16-bit
    /// little-endian samples at 16 kHz mono. For the wake word.
    pub fn buffered(&self, endpoint: &str) -> Vec<u8> {
        self.locked()
            .iter()
            .filter(|e| e.endpoint == endpoint)
            .flat_map(|e| e.buffer.iter().copied())
            .collect()
    }

    /// [`Voice::buffered`], and the buffers are left wiped.
    pub fn take(&self, endpoint: &str) -> Vec<u8> {
        let mut out = Vec::new();
        for entry in self.locked().iter_mut().filter(|e| e.endpoint == endpoint) {
            out.extend(entry.buffer.iter().copied());
            entry.wipe();
        }
        out
    }

    /// Frames dropped for `reason` since this server started.
    pub fn dropped(&self, reason: DropReason) -> u64 {
        self.dropped[reason.index()].load(Ordering::Relaxed)
    }

    /// Frames dropped for any reason since this server started.
    pub fn dropped_frames(&self) -> u64 {
        DropReason::ALL.iter().map(|r| self.dropped(*r)).sum()
    }

    /// Frames kept since this server started.
    pub fn buffered_frames(&self) -> u64 {
        self.buffered.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chorus_protocol::v2::mic_format;

    fn frame(sequence: u32, byte: u8, samples: usize) -> MicAudio {
        MicAudio {
            format: mic_format::PCM_S16LE_16K_MONO,
            sequence,
            timestamp_ns: 0,
            data: vec![byte; samples * MIC_BYTES_PER_SAMPLE],
        }
    }

    const ON: Option<RoomVoice<'static>> = Some(RoomVoice {
        room: "kitchen",
        enabled: true,
    });
    const DISABLED: Option<RoomVoice<'static>> = Some(RoomVoice {
        room: "kitchen",
        enabled: false,
    });

    #[test]
    fn a_frame_is_kept_only_with_the_role_a_room_voice_on_and_a_live_gate() {
        let voice = Voice::new();
        voice.session_up(1, "mic", true);
        voice.session_up(2, "plain", false);
        // Nothing said yet: muted.
        assert_eq!(
            voice.audio(1, ON, &frame(0, 7, 320)).0,
            Intake::Dropped(DropReason::GateMuted)
        );
        assert_eq!(voice.gate(1, true).map(|g| g.0), Some(true));
        assert_eq!(
            voice.audio(1, DISABLED, &frame(1, 7, 320)).0,
            Intake::Dropped(DropReason::VoiceDisabled)
        );
        assert_eq!(
            voice.audio(1, None, &frame(2, 7, 320)).0,
            Intake::Dropped(DropReason::NoRoom)
        );
        assert!(voice.buffered("mic").is_empty());
        assert_eq!(voice.audio(1, ON, &frame(3, 7, 320)).0, Intake::Buffered);
        assert_eq!(voice.buffered("mic"), vec![7u8; 640]);
        // A session without the role is never listened to, and its word
        // about a gate is not taken.
        assert_eq!(voice.gate(2, true), None);
        assert_eq!(
            voice.audio(2, ON, &frame(0, 9, 320)).0,
            Intake::Dropped(DropReason::NoVoiceRole)
        );
        assert_eq!(
            voice.audio(99, ON, &frame(0, 9, 320)).0,
            Intake::Dropped(DropReason::NoVoiceRole)
        );
        assert!(voice.buffered("plain").is_empty());
        assert_eq!(voice.buffered_frames(), 1);
        assert_eq!(voice.dropped(DropReason::GateMuted), 1);
        assert_eq!(voice.dropped(DropReason::VoiceDisabled), 1);
        assert_eq!(voice.dropped(DropReason::NoRoom), 1);
        assert_eq!(voice.dropped(DropReason::NoVoiceRole), 2);
        assert_eq!(voice.dropped_frames(), 5);
    }

    #[test]
    fn the_buffer_is_bounded_and_keeps_the_newest() {
        let voice = Voice::new();
        voice.session_up(1, "mic", true);
        voice.gate(1, true);
        // Four seconds of 100 ms frames, each filled with its own number.
        for n in 0..40u32 {
            assert_eq!(
                voice.audio(1, ON, &frame(n, n as u8, 1600)).0,
                Intake::Buffered
            );
        }
        let held = voice.buffered("mic");
        assert_eq!(held.len(), BUFFER_BYTES);
        assert_eq!(held.first(), Some(&10), "the oldest second fell off");
        assert_eq!(held.last(), Some(&39));
    }

    #[test]
    fn muting_disabling_and_ending_each_wipe_the_buffer() {
        let voice = Voice::new();
        voice.session_up(1, "mic", true);
        voice.gate(1, true);
        voice.audio(1, ON, &frame(0, 7, 320));
        assert!(!voice.buffered("mic").is_empty());
        voice.gate(1, false);
        assert!(voice.buffered("mic").is_empty(), "muted");

        voice.gate(1, true);
        voice.audio(1, ON, &frame(0, 7, 320));
        voice.audio(1, DISABLED, &frame(1, 7, 320));
        assert!(
            voice.buffered("mic").is_empty(),
            "a frame of a disabled room"
        );

        voice.audio(1, ON, &frame(0, 7, 320));
        voice.tell(
            1,
            VoiceControl {
                uplink: false,
                listening: false,
            },
        );
        assert!(voice.buffered("mic").is_empty(), "told to stop");

        voice.audio(1, ON, &frame(0, 7, 320));
        assert_eq!(voice.take("mic").len(), 640);
        assert!(voice.buffered("mic").is_empty(), "taken");

        voice.audio(1, ON, &frame(0, 7, 320));
        let (still_live, line) = voice.session_down(1);
        assert!(!still_live);
        assert!(line
            .unwrap()
            .contains("ended buffered_frames=5 dropped_frames=1"));
        assert!(voice.buffered("mic").is_empty(), "ended");
        assert_eq!(voice.told(1), None);
    }

    #[test]
    fn a_status_line_is_one_per_change_and_names_counts_not_samples() {
        let voice = Voice::new();
        voice.session_up(1, "mic", true);
        let (_, line) = voice.audio(1, ON, &frame(0, 0x4d, 320));
        assert_eq!(
            line.as_deref(),
            Some(
                "voice mic id=mic room=kitchen intake=dropping reason=gate-muted \
                 buffered_frames=0 dropped_frames=1"
            )
        );
        assert_eq!(voice.audio(1, ON, &frame(1, 0x4d, 320)).1, None);
        let (_, line) = voice.gate(1, true).unwrap();
        assert_eq!(
            line.as_deref(),
            Some("voice mic id=mic gate=live buffered_frames=0 dropped_frames=2")
        );
        let (_, line) = voice.audio(1, ON, &frame(2, 0x4d, 320));
        assert_eq!(
            line.as_deref(),
            Some(
                "voice mic id=mic room=kitchen intake=buffering buffered_frames=1 \
                 dropped_frames=2"
            )
        );
        assert_eq!(voice.audio(1, ON, &frame(3, 0x4d, 320)).1, None);
        assert!(
            !format!("{:?}", voice).contains("77"),
            "no sample is printed"
        );
    }

    #[test]
    fn a_reconnecting_endpoints_gate_is_its_live_sessions() {
        let voice = Voice::new();
        voice.session_up(1, "mic", true);
        voice.session_up(2, "mic", true);
        voice.gate(1, true);
        voice.gate(2, true);
        assert!(voice.session_down(1).0, "the other session is still live");
        assert!(!voice.session_down(2).0);
    }
}
