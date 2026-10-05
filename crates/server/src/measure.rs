//! The measurement sweep: the room-correction sweep played in one room,
//! once, and the room given back to what its group plays (ADR 0195; the
//! playback half of the phone-microphone measurement,
//! `docs/room-correction.md`).
//!
//! The `measure_sweep` command names a room and, optionally, a volume. The
//! sweep is the one the fitter deconvolves with
//! (`chorus_dsp::roomfit::Sweep::recommended` at the stream's rate), and it
//! reaches the room on a stream of its own (`crate::sweep`, cut by the audio
//! thread beside the slots): the conductor routes the room's player sessions
//! to that stream for as long as the sweep's program lasts and back to
//! their group's slot afterwards. So no group's source changes, a room that
//! shares a group with others is measured alone while the others keep
//! playing, and "giving the room back" is one move of its sessions.
//!
//! # The level
//!
//! The sweep's samples are the fitter's (half full scale); how loud the room
//! plays them is the room's volume. With `volume` the room is set to it for
//! the sweep through the room model's one volume path, which clamps to the
//! room's effective limit (its own limit and every active quiet-hours
//! window), and afterwards it goes back to the volume it had unless somebody
//! changed it meanwhile. Without `volume` the room plays the sweep at its
//! own. Either way the level is never above the effective limit, and a
//! limit lowered while the sweep plays is in force at the next
//! `room_volume`, as for everything else the room plays.
//!
//! # How it ends
//!
//! The state names the sweep under `measurement`: `playing` from the
//! command, then `finished` (the program played to its last frame) or
//! `cancelled` with a reason (an alarm rings in the room, the room's last
//! player session went away, or the audio thread did not report the end).
//! A server plays one sweep at a time: it has one sweep stream.
//!
//! Two threads use it. A control worker starts a sweep, inside the command,
//! so a refusal is the command's answer. The conductor routes, starts and
//! ends it ([`Measurer::route`], [`Measurer::direct`], [`Measurer::settle`],
//! once per pass). One mutex holds the sweep in progress; the room model's
//! lock is taken only inside the calls made under it, never the other way
//! round.
//!
//! Control code, off the audio path: `audio-path.conf` records it as
//! excluded. It touches no PCM; what it sends the audio thread is a
//! [`SweepCommand`]. The one clock it reads is the monotonic one, for the
//! bound on how long it waits for the audio thread to report the end.

use std::sync::mpsc::SyncSender;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use chorus_control::catalog::{Refusal, Volume};
use chorus_control::zones::MeasurementState;
use chorus_protocol::v2::roles;

use crate::control::ControlState;
use crate::router::Router;
use crate::slots::SlotCommand;
use crate::sweep::{SweepCommand, SweepPort};

/// How long past the program's length the conductor waits for the audio
/// thread to say the sweep completed before it calls the sweep off.
/// ASSUMED: five seconds, far above the conductor's own pace (a pass at
/// least every 200 ms) and a full command queue's retry (50 ms), so it only
/// ever ends a sweep the audio thread will never report.
pub const END_GRACE: Duration = Duration::from_secs(5);

/// One sweep that ended, for the schedule runtime: a room it holds must not
/// be put back at the sweep's volume.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ended {
    /// The room.
    pub zone: String,
    /// The volume it had.
    pub before: Volume,
    /// The volume the sweep gave it.
    pub set: Volume,
}

#[derive(Debug)]
struct Live {
    id: u64,
    zone: String,
    before: Volume,
    set: Volume,
    since: Instant,
    /// Whether the audio thread was told to start this play.
    started: bool,
    /// Set once the sweep is called off: the audio thread is still owed its
    /// `Cancel`.
    cancelled: bool,
}

/// The server's measurement sweeps.
pub struct Measurer {
    router: Arc<Router>,
    commands: SyncSender<SlotCommand>,
    port: Arc<SweepPort>,
    /// The lead silence, the sweep and the tail silence, ms.
    lengths_ms: (u64, u64, u64),
    live: Mutex<Option<Live>>,
    log: Box<dyn Fn(&str) + Send + Sync>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    match m.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    }
}

impl Measurer {
    /// Sweeps on `router`'s sweep stream, whose player is told through
    /// `commands` (the slots' own command channel) and says what it did on
    /// `port`. `lengths_ms` is the program: the lead silence, the sweep and
    /// the tail silence. `log` gets one line per start and end.
    pub fn new(
        router: Arc<Router>,
        commands: SyncSender<SlotCommand>,
        port: Arc<SweepPort>,
        lengths_ms: (u64, u64, u64),
        log: Box<dyn Fn(&str) + Send + Sync>,
    ) -> Measurer {
        Measurer {
            router,
            commands,
            port,
            lengths_ms,
            live: Mutex::new(None),
            log,
        }
    }

    /// Whether a sweep is playing or stopping.
    pub fn any_live(&self) -> bool {
        lock(&self.live).is_some()
    }

    /// Whether a player session of room `zone` is up: somebody to play the
    /// sweep.
    fn has_player(&self, state: &ControlState, zone: &str) -> bool {
        let snapshot = state.snapshot();
        self.router.sessions().iter().any(|session| {
            session.roles & roles::PLAYER != 0
                && snapshot
                    .room_of(&session.endpoint)
                    .is_some_and(|room| room.id == zone)
        })
    }

    /// Carry out one `measure_sweep` command on `state`. Returns the state
    /// as it stands once the sweep has started (`measurement` names it,
    /// `playing`); refused by name, with nothing changed, as the room model
    /// refuses (`Zones::measure_check`: an unknown room, `measuring`,
    /// `no-speaker`, `muted`, `alarm-ringing`), and with `no-speaker` for a
    /// room none of whose speakers has a player session up.
    pub fn measure(
        &self,
        state: &ControlState,
        zone: &str,
        volume: Option<Volume>,
    ) -> Result<String, Refusal> {
        let mut live = lock(&self.live);
        state.measure_check(zone)?;
        if !self.has_player(state, zone) {
            return Err(Refusal::rejected(
                "zone",
                format!(
                    "no-speaker: no speaker of room '{}' has a player session up, so the \
                     server cannot play a sweep there",
                    zone
                ),
            ));
        }
        if let Some(stopping) = live.as_ref() {
            // The room model says no sweep is playing, so this one was
            // called off and the audio thread has not been told yet.
            return Err(Refusal::rejected(
                "zone",
                format!(
                    "measuring: measurement sweep {} in room '{}' is still stopping; send the \
                     command again",
                    stopping.id, stopping.zone
                ),
            ));
        }
        let (measured, answer) = state.measure_begin(zone, volume, self.lengths_ms)?;
        (self.log)(&format!(
            "measure sweep id={} zone={} volume={} asked={} before={} lead_ms={} sweep_ms={} \
             tail_ms={}",
            measured.id,
            zone,
            measured.set.literal(),
            volume.map_or("unchanged".to_string(), |v| v.literal()),
            measured.before.literal(),
            self.lengths_ms.0,
            self.lengths_ms.1,
            self.lengths_ms.2
        ));
        *live = Some(Live {
            id: measured.id,
            zone: zone.to_string(),
            before: measured.before,
            set: measured.set,
            since: Instant::now(),
            started: false,
            cancelled: false,
        });
        Ok(answer)
    }

    /// The room whose player sessions hear the sweep's stream now, for the
    /// conductor's routing: from the command to the end of the program.
    pub fn route(&self) -> Option<String> {
        lock(&self.live)
            .as_ref()
            .filter(|live| !live.cancelled)
            .map(|live| live.zone.clone())
    }

    /// Tell the audio thread to start the sweep, on the conductor's thread,
    /// AFTER the pass has moved the room's sessions onto the sweep's stream
    /// (so they hear the program from its first frame). Returns how many
    /// commands the audio thread's queue did not take; they are sent on the
    /// next pass.
    pub fn direct(&self) -> usize {
        let mut live = lock(&self.live);
        let Some(live) = live.as_mut().filter(|l| !l.started && !l.cancelled) else {
            return 0;
        };
        let start = SlotCommand::Sweep(SweepCommand::Start { id: live.id });
        if self.commands.try_send(start).is_err() {
            return 1;
        }
        live.started = true;
        live.since = Instant::now();
        0
    }

    /// End the sweep when it is over, on the conductor's thread, once per
    /// pass: when the audio thread says its program completed the state
    /// says `finished`; when an alarm rings in its room, the room's last
    /// player session went away or the audio thread did not report the end
    /// in time, the audio thread is told to stop and the state says
    /// `cancelled` and why. Either way the room's volume goes back when it
    /// is still the sweep's. Returns the sweep that ended, for the schedule
    /// runtime to be told, and how many commands the audio thread's queue
    /// did not take.
    pub fn settle(&self, state: &ControlState) -> (Option<Ended>, usize) {
        let mut slot = lock(&self.live);
        let Some(live) = slot.as_mut() else {
            return (None, 0);
        };
        let mut ended = None;
        if !live.cancelled {
            let total =
                Duration::from_millis(self.lengths_ms.0 + self.lengths_ms.1 + self.lengths_ms.2);
            let outcome = if live.started && self.port.completed() == live.id {
                Some((MeasurementState::Finished, None))
            } else if let Some(alarm) = state.measure_watch(&live.zone) {
                Some((
                    MeasurementState::Cancelled,
                    Some(format!("alarm '{}' rings in the room", alarm)),
                ))
            } else if !self.has_player(state, &live.zone) {
                Some((
                    MeasurementState::Cancelled,
                    Some("the room's last player session went away".to_string()),
                ))
            } else if live.since.elapsed() >= total + END_GRACE {
                Some((
                    MeasurementState::Cancelled,
                    Some("the audio thread did not report the sweep's end".to_string()),
                ))
            } else {
                None
            };
            let Some((how, reason)) = outcome else {
                return (None, 0);
            };
            state.measure_end(live.id, how, reason.clone(), live.before, live.set);
            (self.log)(&format!(
                "measure sweep id={} zone={} outcome={}{} volume-back-to={}",
                live.id,
                live.zone,
                how.name(),
                reason.map_or(String::new(), |r| format!(" reason=\"{}\"", r)),
                live.before.literal()
            ));
            ended = Some(Ended {
                zone: live.zone.clone(),
                before: live.before,
                set: live.set,
            });
            if how == MeasurementState::Finished {
                *slot = None;
                return (ended, 0);
            }
            live.cancelled = true;
        }
        // Called off: the audio thread stops at its next chunk boundary. A
        // play it was never told to start has nothing to stop.
        let cancel = SlotCommand::Sweep(SweepCommand::Cancel { id: live.id });
        if live.started && self.commands.try_send(cancel).is_err() {
            return (ended, 1);
        }
        *slot = None;
        (ended, 0)
    }
}
