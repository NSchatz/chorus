//! The conductor: every change to the room model, carried to the sessions it
//! concerns.
//!
//! # What it does
//!
//! One thread, created with the rest of the population before the scheduling
//! report (`crates/server/src/main.rs`). It wakes when the control plane
//! commits a change (a command, an endpoint's button, a session coming up:
//! `ControlState::wake_conductor`), reads the room model once
//! (`ControlState::snapshot`), and makes the audio sessions agree with it:
//!
//! - **routing** (`--slots S`): what each stream slot plays
//!   ([`SlotCommand`] to the audio thread, at its next chunk boundary), and
//!   which slot each session hears (`Router::move_to`, between two ticks);
//! - **`room_volume`** to every player session of a room whose gain (volume,
//!   mute) or effective limit changed (docs/decisions/0074-*);
//! - **`controller_state`** to every controller session of a room whose
//!   volume, mute or group changed, wherever the change was made: the page, an
//!   endpoint's buttons, another endpoint (the ADR 0067 follow-up).
//!
//! Each push is deduped against what the session was last sent
//! (`crate::router`), so a change concerning another room sends nothing, and a
//! push a full queue refused is left owed and tried again on the next pass,
//! [`RETRY`] later.
//!
//! # No clock, and the seam for the one that comes next
//!
//! In this change the conductor reads NO clock: it is change-driven, and
//! every value it sends was computed by the room model from commands. The
//! next goal 11 track adds the time-driven work (alarms, sleep timers, the
//! quiet-hours clock, ramps) through a pure module, `schedule_runtime.rs`,
//! whose `tick` returns effects. It plugs in at exactly two places, both named
//! here so that change is small:
//!
//! - [`Conductor::wait`] is the loop's wake: today a poke or [`IDLE`]; there
//!   the earlier of that and the runtime's next deadline.
//! - [`Conductor::pass`] is the effect application: today it applies the room
//!   model to the sessions; there it first applies the runtime's effects to
//!   the room model through `ControlState`'s runtime hooks (each of which
//!   wakes this thread again), then does what it does now.
//!
//! Control code: it touches no PCM and stamps nothing.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, SyncSender, TrySendError};
use std::sync::Arc;
use std::time::Duration;

use chorus_protocol::v2::roles;

use crate::control::{ControlState, Snapshot};
use crate::router::Router;
use crate::slots::{SlotCommand, SlotInput};

/// How long the conductor waits for a poke before it looks up to see whether
/// it is still wanted.
pub const IDLE: Duration = Duration::from_millis(200);

/// How soon a pass that left something owed (a full queue, a full slot
/// channel) is run again. ASSUMED: a few chunk durations at the default
/// 20 ms; a queue that full is 128 items behind already.
pub const RETRY: Duration = Duration::from_millis(50);

/// What one pass did, for the tests and a status line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PassReport {
    /// `room_volume` messages sent.
    pub room_volumes: usize,
    /// `controller_state` messages sent.
    pub controller_states: usize,
    /// Sessions moved between fanouts.
    pub moves: usize,
    /// Slot inputs changed.
    pub inputs: usize,
    /// Pushes owed to a full queue, to try again.
    pub owed: usize,
}

/// The conductor's state between passes.
pub struct Conductor {
    state: Arc<ControlState>,
    router: Arc<Router>,
    slots: Option<SyncSender<SlotCommand>>,
    /// What each slot was last told to play.
    inputs: Vec<SlotInput>,
}

impl Conductor {
    /// A conductor over `state`'s room model and `router`'s sessions, telling
    /// the audio thread what each slot plays over `slots` (`None` in the
    /// one-stream shape, which has no slot to tell).
    pub fn new(
        state: Arc<ControlState>,
        router: Arc<Router>,
        slots: Option<SyncSender<SlotCommand>>,
    ) -> Conductor {
        let inputs = vec![SlotInput::Silence; router.slots()];
        Conductor {
            state,
            router,
            slots,
            inputs,
        }
    }

    /// The loop's wake (the seam the schedule runtime's deadline joins):
    /// `true` when woken or timed out and still wanted, `false` when the run
    /// is stopping.
    pub fn wait(&self, woken: &Receiver<()>, owed: bool, keep: &AtomicBool) -> bool {
        match woken.recv_timeout(if owed { RETRY } else { IDLE }) {
            Ok(()) | Err(RecvTimeoutError::Timeout) => keep.load(Ordering::SeqCst),
            Err(RecvTimeoutError::Disconnected) => false,
        }
    }

    /// One pass (the seam the schedule runtime's effects join): read the
    /// room model once and make every session agree with it.
    pub fn pass(&mut self) -> PassReport {
        let snapshot = self.state.snapshot();
        let mut report = PassReport::default();
        self.route_inputs(&snapshot, &mut report);
        for session in self.router.sessions() {
            let room = snapshot.room_of(&session.endpoint);
            let route = room
                .and_then(|r| r.route)
                .unwrap_or_else(|| self.router.idle());
            if self.router.slots() > 0 && self.router.move_to(session.id, route) {
                report.moves += 1;
            }
            let Some(room) = room else {
                continue;
            };
            if session.roles & roles::PLAYER != 0 {
                match self.router.push_room_volume(session.id, room.room_volume) {
                    Some(true) => report.room_volumes += 1,
                    Some(false) => {}
                    None => report.owed += 1,
                }
            }
            if session.roles & roles::CONTROLLER != 0 {
                match self
                    .router
                    .push_controller_state(session.id, &room.controller_state, false)
                {
                    Some(true) => report.controller_states += 1,
                    Some(false) => {}
                    None => report.owed += 1,
                }
            }
        }
        report
    }

    fn route_inputs(&mut self, snapshot: &Snapshot, report: &mut PassReport) {
        let Some(slots) = &self.slots else {
            return;
        };
        for (slot, wanted) in snapshot.inputs.iter().enumerate() {
            if self.inputs.get(slot) == Some(wanted) {
                continue;
            }
            match slots.try_send(SlotCommand::Input {
                slot,
                input: *wanted,
            }) {
                Ok(()) => {
                    self.inputs[slot] = *wanted;
                    report.inputs += 1;
                }
                Err(TrySendError::Full(_)) => report.owed += 1,
                Err(TrySendError::Disconnected(_)) => {}
            }
        }
    }
}

/// The `conductor` thread's loop. Returns when `keep` says stop.
pub fn run(mut conductor: Conductor, keep: Arc<AtomicBool>) {
    let Some(woken) = conductor.state.take_conductor_wake() else {
        return;
    };
    // The first pass sets every slot's input from the model as it starts.
    let mut owed = conductor.pass().owed > 0;
    while conductor.wait(&woken, owed, &keep) {
        owed = conductor.pass().owed > 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    use chorus_control::rooms::{CivilTime, ClockTime};
    use chorus_control::zones::{Zone, Zones};
    use chorus_protocol::v2::{decode_frame, Message, Outcome, RoomVolume};

    use crate::stream::{Outbound, SUBSCRIBER_QUEUE_LIMIT};

    fn kitchen() -> Arc<ControlState> {
        let mut zones = Zones::new("127.0.0.1:4010");
        let mut zone = Zone::new("kitchen");
        zone.endpoints.push("speaker".to_string());
        zones.add(zone).unwrap();
        Arc::new(ControlState::new(zones, None))
    }

    fn room_volumes(inbox: &mpsc::Receiver<Outbound>) -> Vec<RoomVolume> {
        inbox
            .try_iter()
            .filter_map(|o| match o {
                Outbound::Frame(bytes) => match decode_frame(&bytes).outcome {
                    Outcome::Decoded(Message::RoomVolume(m)) => Some(m),
                    _ => None,
                },
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_quiet_window_starting_at_the_civil_time_given_is_pushed_once_and_clamped() {
        let state = kitchen();
        let router = Arc::new(Router::single(Arc::new(crate::stream::Fanout::new())));
        let (out, inbox) = mpsc::sync_channel(SUBSCRIBER_QUEUE_LIMIT);
        let start = state.session_start("speaker", roles::PLAYER, router.idle());
        assert_eq!(
            start.room_volume,
            Some(RoomVolume {
                gain: 1000,
                limit: 1000,
                ramp_ms: 0
            })
        );
        router.register("speaker", roles::PLAYER, out, &start);
        let mut conductor = Conductor::new(Arc::clone(&state), Arc::clone(&router), None);
        assert_eq!(
            conductor.pass().room_volumes,
            0,
            "the greeting said it already"
        );
        state
            .apply(
                r#"{"v":2,"t":"quiet_hours","zone":"kitchen","windows":[{"days":["mon"],"start":"22:00","end":"07:00","limit":0.200}]}"#,
            )
            .unwrap();
        assert_eq!(
            conductor.pass().room_volumes,
            0,
            "no civil time yet: no window is active"
        );
        let late = CivilTime {
            weekday: 0,
            time: ClockTime::parse("23:30").unwrap(),
        };
        assert!(state.set_civil_time(Some(late)));
        assert_eq!(conductor.pass().room_volumes, 1);
        assert_eq!(conductor.pass().room_volumes, 0, "deduped");
        assert_eq!(
            room_volumes(&inbox),
            vec![RoomVolume {
                gain: 200,
                limit: 200,
                ramp_ms: 0
            }]
        );
    }
}
