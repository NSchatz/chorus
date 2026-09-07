//! AC-4's modelled half: 72 hours, and every resync with a name on it.
//!
//! **THIS IS A MODELLED RESULT. IT IS NOT A MEASUREMENT AND IT IS NOT AC-4.**
//! AC-4 asks for a system left alone for three days of wall clock, holding its
//! sync bound as the RIG-3 rig measures it. Neither the three days nor the rig
//! exists in this repository, `make verify-soak` refuses by name because of it,
//! and `docs/verification-record.md` quotes that refusal. What is here is what
//! CAN be run: the same sync loop, the same corrector, the same control
//! catalog, the same zone state and the same fanout, driven for 72 modelled
//! hours against modelled clocks, a modelled network and a modelled DAC.
//!
//! # What is real in it and what is modelled
//!
//! Real: `SyncLoop`, `PlayoutCorrector`, `chorus_control::Zones`,
//! `chorus_control::ControlFanout`, `decode_command`, `ZoneWatch` and
//! `ZoneGain` are the types the shipped binaries use, unaltered. The session
//! supervisor's behaviour - a session ends, the endpoint rejoins, the zone
//! state it comes back to is the one that was persisted - is driven through the
//! same `Zones` and the same persisted format the server writes.
//!
//! Modelled: the clocks, the network delay, the DAC, and the passage of time.
//! Nothing here waits for anything.
//!
//! # The resync classification
//!
//! The second half of AC-4 is "SHALL report no unexplained resync", so every
//! hard resync this run produces is matched against the events that were
//! injected, and one that matches nothing is COUNTED AND REPORTED as
//! unexplained. The count is asserted to be zero; if a future change makes the
//! loop resync for a reason nobody scheduled, this goes red naming the hour.

mod common;

use std::collections::BTreeMap;
use std::sync::Arc;

use chorus_client_linux::control::ZoneWatch;
use chorus_client_linux::sync::SyncConfig;
use chorus_client_linux::zone::ZoneGain;
use chorus_control::catalog::decode_command;
use chorus_control::fanout::ControlFanout;
use chorus_control::persist;
use chorus_control::zones::{Zone, Zones};
use chorus_protocol::SampleFormat;

use common::{ModelParams, ModelledEndpoint};

/// The window AC-4 names, in modelled hours.
///
/// The full 72, on every run of the ordinary suite: 13 million modelled steps,
/// which take about a quarter of a minute. The environment variable exists so
/// that a shorter window can be run while working on this file, and the window
/// that was used is PRINTED in the report below, so a figure from a short run
/// can never be mistaken for one from the full one.
const DEFAULT_HOURS: u64 = 72;

/// How many modelled sessions the window is split into.
///
/// A soak whose server never restarts would not exercise what PRODUCT-6 added,
/// and one that restarted it every few minutes would be measuring acquisition
/// rather than the steady state.
const SESSIONS: u64 = 4;

fn soak_hours() -> u64 {
    match std::env::var("CHORUS_MODELLED_SOAK_HOURS") {
        Ok(text) => text
            .parse()
            .unwrap_or_else(|_| panic!("CHORUS_MODELLED_SOAK_HOURS='{}' is not a number", text)),
        Err(_) => DEFAULT_HOURS,
    }
}

/// The first minute of each session is acquisition and is excluded, exactly as
/// SYNC-4's own modelled hour excludes it and for the same reason: the run is
/// deliberately started outside the bound.
const SETTLE_NS: u64 = 60 * 1_000_000_000;

/// The bound the MODELLED error is held to.
///
/// The same 0.25 ms `docs/verification-record.md` records SYNC-4's modelled
/// hour against, and half of the 0.5 ms AC-1 of `chorus#SYNC-4` asks a real rig
/// for. It is a modelled bound on a modelled quantity and it says nothing about
/// two loudspeakers in a room.
const MODELLED_BOUND_NS: f64 = 250_000.0;

/// One chunk duration per step.
///
/// Coarser than the 5 ms `sync_loop.rs` uses for an hour, and stated rather
/// than hidden: this run says whether the loop stays inside the bound over
/// three modelled days at chunk granularity, and NOT what the error does
/// between one chunk and the next. `--test sync_loop` answers the second
/// question over an hour at 5 ms.
///
/// It cannot be made much coarser. The modelled DAC drains on the virtual
/// clock, so a step longer than the 120 ms the pacing holds would empty the
/// ring before the next write and the run would be measuring an underrun
/// cascade rather than a sync loop.
const STEP_NS: u64 = 20_000_000;

/// Why a hard resync happened, decided from the events that were injected and
/// not from anything the loop said about itself.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Cause {
    /// The acquisition transient at the start of the first session.
    Acquisition,
    /// The acquisition transient after a modelled server restart.
    AcquisitionAfterRestart,
    /// No scheduled event accounts for it.
    Unexplained,
}

impl Cause {
    fn name(&self) -> &'static str {
        match self {
            Cause::Acquisition => "acquisition",
            Cause::AcquisitionAfterRestart => "acquisition-after-a-server-restart",
            Cause::Unexplained => "UNEXPLAINED",
        }
    }
}

/// The classifier itself, in one place.
///
/// It is a named function rather than an expression inside the run's loop for
/// one reason: the demonstration that it CAN say "unexplained" has to drive the
/// same code the run drives. A demonstration that restates the rule in its own
/// body proves that the person who wrote the test can write an `if`, and
/// nothing at all about the classification the record cites.
///
/// A resync is explained only by an event the run SCHEDULED - the acquisition
/// transient at the start of a session, which is the settle window. Anything
/// outside a scheduled window is unexplained BY CONSTRUCTION, which is the
/// property AC-4's second half asks for: the answer does not depend on the
/// loop's own account of itself.
fn classify(at_ns: u64, session_index: usize) -> Cause {
    if at_ns <= SETTLE_NS {
        if session_index == 0 {
            Cause::Acquisition
        } else {
            Cause::AcquisitionAfterRestart
        }
    } else {
        Cause::Unexplained
    }
}

/// Every hard resync of one session, classified.
fn classify_all(hard_resyncs_at_ns: &[u64], session_index: usize) -> Vec<(u64, Cause)> {
    hard_resyncs_at_ns
        .iter()
        .map(|at| (*at, classify(*at, session_index)))
        .collect()
}

/// How many of them no scheduled event accounts for. This is the figure the
/// run reports and asserts is zero.
fn count_unexplained(resyncs: &[(u64, Cause)]) -> usize {
    resyncs
        .iter()
        .filter(|(_, cause)| *cause == Cause::Unexplained)
        .count()
}

/// What one modelled session did.
struct Session {
    index: usize,
    resyncs: Vec<(u64, Cause)>,
    max_true_error_after_settle_ns: f64,
    underruns: u64,
    accepted: u64,
    discarded: u64,
    inserted_frames: u64,
    dropped_frames: u64,
}

#[test]
fn seventy_two_modelled_hours_hold_the_bound_and_every_resync_has_a_name() {
    let soak_hours = soak_hours();
    assert_eq!(
        soak_hours % SESSIONS,
        0,
        "the window has to divide into {} sessions",
        SESSIONS
    );
    let session_hours = soak_hours / SESSIONS;
    let sessions_wanted = SESSIONS as usize;
    let session_ns = session_hours * 3_600 * 1_000_000_000;
    let sync = SyncConfig::default();

    // The real control plane, driven across the whole soak. The zone state is
    // persisted and reloaded at each modelled restart, through the same code
    // the server uses, so "what the endpoints come back to" is what a restart
    // actually gives them.
    let mut zones = Zones::new("127.0.0.1:4010");
    zones.add(Zone::new("kitchen")).expect("a zone");
    zones.add(Zone::new("study")).expect("a zone");
    let fanout = ControlFanout::new();
    let watcher = fanout.subscribe();
    let endpoint_watch = Arc::new(ZoneWatch::new());
    let gain = ZoneGain::new(SampleFormat::PcmS16Le);

    let mut commands_applied = 0u64;
    let mut states_seen = 0u64;
    let mut samples_scaled = 0u64;
    let mut persisted = String::new();

    let mut sessions: Vec<Session> = Vec::new();
    for index in 0..sessions_wanted {
        // A modelled server restart between sessions: the state is written by
        // the server and read back by the one that replaces it, with nothing
        // else carried over.
        if index > 0 {
            let reloaded = persist::load(&persisted, "127.0.0.1:4010")
                .expect("the persisted state reads back");
            assert_eq!(
                persist::render(&reloaded),
                persisted,
                "session {}: the state a restart gives back is not the state that was written",
                index
            );
            zones = reloaded;
        }

        // A different draw of network jitter and a different crystal pair per
        // session, so that four sessions are four different eighteen hours and
        // not one eighteen hours reported four times. The crystals stay inside
        // the +-50 ppm pair `fixtures/sync/03-worst-case-skew.cfg` treats as
        // the worst case.
        let spread = [(0.0, 40.0), (-12.5, 38.0), (50.0, -50.0), (-30.0, 25.0)][index];
        let mut model = ModelledEndpoint::new(
            ModelParams {
                step_ns: STEP_NS,
                seed: 0x5EED_5111u64.wrapping_add((index as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)),
                server_ppm: spread.0,
                client_ppm: spread.1,
                ..Default::default()
            },
            sync,
        );
        // Every session starts outside the bound, which is what a real endpoint
        // joining a stream does and which is what makes the acquisition resync
        // a scheduled event rather than a surprise.
        model.initial_misalignment_ns = 4_000_000;
        let result = model.run(session_ns);

        // The control plane, exercised on this session's modelled hours: one
        // command per modelled hour, applied through the real catalog and the
        // real zone state, fanned out on the real fanout, absorbed by the real
        // endpoint-side watch, and applied to real PCM by the real gain.
        for hour in 0..session_hours {
            let thousandths = 200 + (hour % 8) * 100;
            let text = format!(
                r#"{{"v":1,"t":"volume","zone":"kitchen","volume":0.{:03}}}"#,
                thousandths
            );
            let command = decode_command(&text).unwrap_or_else(|e| panic!("{}: {}", text, e));
            zones.apply(&command).expect("the zone exists");
            commands_applied += 1;
            if hour % 6 == 0 {
                let muted = (hour / 6) % 2 == 1;
                let text = format!(
                    r#"{{"v":1,"t":"mute","zone":"study","muted":{}}}"#,
                    muted
                );
                zones
                    .apply(&decode_command(&text).expect("a mute"))
                    .expect("the zone exists");
                commands_applied += 1;
            }
            let state = zones.encode_state();
            assert_eq!(
                fanout.broadcast(Arc::new(state.clone())),
                1,
                "the subscriber was dropped at hour {} of session {}",
                hour,
                index
            );
            let held = watcher.recv().expect("the subscriber is attached");
            assert_eq!(*held, state, "a subscriber was sent something else");
            states_seen += 1;
            assert!(endpoint_watch.absorb(&held, "kitchen"));

            // And the gain reaches PCM, every modelled hour, so the control
            // plane's effect is exercised for the whole run and not once.
            let mut pcm: Vec<u8> = std::iter::repeat(16_000i16.to_le_bytes())
                .take(480)
                .flatten()
                .collect();
            gain.apply(endpoint_watch.gain(), &mut pcm);
            let expected = (16_000i64 * i64::from(endpoint_watch.gain().thousandths()) / 1_000) as i16;
            for sample in pcm.chunks_exact(2) {
                assert_eq!(
                    i16::from_le_bytes([sample[0], sample[1]]),
                    expected,
                    "the gain in force did not reach the samples at hour {} of session {}",
                    hour,
                    index
                );
                samples_scaled += 1;
            }
        }
        persisted = persist::render(&zones);

        let resyncs = classify_all(&result.hard_resyncs_at_ns, index);

        sessions.push(Session {
            index,
            resyncs,
            max_true_error_after_settle_ns: result.max_true_error_after(SETTLE_NS),
            underruns: result.underruns,
            accepted: result.accepted,
            discarded: result.discarded,
            inserted_frames: result.inserted_frames,
            dropped_frames: result.dropped_frames,
        });
    }

    // --- the report -----------------------------------------------------------
    //
    // Printed rather than only asserted, so that every figure in
    // docs/verification-record.md is one a reader can produce rather than take.

    println!(
        "soak: MODELLED {} hours, in {} sessions of {} hours",
        soak_hours, sessions_wanted, session_hours
    );
    if soak_hours != DEFAULT_HOURS {
        println!(
            "soak: NOTE CHORUS_MODELLED_SOAK_HOURS shortened this run to {} hours; the record \
             reports the {}-hour one",
            soak_hours, DEFAULT_HOURS
        );
    }
    println!(
        "soak: step={} ms, settle={} s, modelled bound={} us",
        STEP_NS / 1_000_000,
        SETTLE_NS / 1_000_000_000,
        MODELLED_BOUND_NS / 1_000.0
    );

    let mut by_cause: BTreeMap<String, usize> = BTreeMap::new();
    let mut unexplained = 0usize;
    let mut worst = 0.0f64;
    for session in &sessions {
        worst = worst.max(session.max_true_error_after_settle_ns);
        println!(
            "soak: session {} peak_true_error_after_settle_us={:.1} underruns={} accepted={} \
             discarded={} inserted_frames={} dropped_frames={} resyncs={}",
            session.index,
            session.max_true_error_after_settle_ns / 1_000.0,
            session.underruns,
            session.accepted,
            session.discarded,
            session.inserted_frames,
            session.dropped_frames,
            session.resyncs.len()
        );
        for (at, cause) in &session.resyncs {
            println!(
                "soak:   resync session={} at_hour={:.3} cause={}",
                session.index,
                *at as f64 / 3_600e9,
                cause.name()
            );
            *by_cause.entry(cause.name().to_string()).or_default() += 1;
        }
        unexplained += count_unexplained(&session.resyncs);
        assert_eq!(
            session.underruns, 0,
            "session {} underran, which is what three days of unattended running must not do",
            session.index
        );
        assert!(
            session.max_true_error_after_settle_ns < MODELLED_BOUND_NS,
            "session {} peaked at {:.1} us after the settle, against a modelled bound of {:.1} us",
            session.index,
            session.max_true_error_after_settle_ns / 1_000.0,
            MODELLED_BOUND_NS / 1_000.0
        );
    }
    for (cause, count) in &by_cause {
        println!("soak: resyncs by cause: {} = {}", cause, count);
    }
    println!(
        "soak: unexplained_resyncs={} commands_applied={} states_fanned_out={} samples_scaled={}",
        unexplained, commands_applied, states_seen, samples_scaled
    );
    println!(
        "soak: control fanout: {} ; dropped_subscribers={} dropped_messages={}",
        fanout.report(),
        fanout.dropped_subscribers(),
        fanout.dropped_messages()
    );
    println!("soak: worst modelled error after any settle = {:.1} us", worst / 1_000.0);
    println!("soak: THIS IS A MODELLED RESULT. It is not a measurement and it is not AC-4.");

    // --- the assertions -------------------------------------------------------

    assert_eq!(
        unexplained, 0,
        "the second half of AC-4 is 'no unexplained resync', and {} of the {} resyncs in this \
         modelled run match no scheduled event",
        unexplained,
        by_cause.values().sum::<usize>()
    );
    assert_eq!(
        by_cause.get("acquisition").copied().unwrap_or(0),
        1,
        "the first session's acquisition is one resync, and the classification has to see it"
    );
    assert_eq!(
        by_cause
            .get("acquisition-after-a-server-restart")
            .copied()
            .unwrap_or(0),
        sessions_wanted - 1,
        "every modelled restart has to produce exactly one acquisition, or the restart is not \
         being exercised"
    );
    assert_eq!(fanout.dropped_subscribers(), 0);
    assert_eq!(fanout.dropped_messages(), 0);
    assert_eq!(
        states_seen, soak_hours,
        "one state message per modelled hour, every one of them delivered"
    );
    assert!(commands_applied >= soak_hours);
    assert!(samples_scaled > 0);
}

#[test]
fn the_classification_reports_a_resync_it_cannot_account_for() {
    // The demonstration the criterion's second half needs: the classifier is
    // only worth having if it can say "unexplained". A resync in the middle of
    // a session, with nothing scheduled anywhere near it, is exactly that.
    //
    // It drives `classify_all` and `count_unexplained`, which are the SAME two
    // functions the modelled run above drives, on a resync list of the same
    // shape as `result.hard_resyncs_at_ns`. Nothing here restates the rule: an
    // edit that made the classifier unable to answer "unexplained" would turn
    // this red, and an edit that made it answer "unexplained" for a scheduled
    // acquisition would turn it red the other way.
    let mid_session = 9 * 3_600 * 1_000_000_000u64;
    assert!(
        mid_session > SETTLE_NS,
        "the smuggled resync has to be outside the settle window to be unaccounted for"
    );

    let first_session = classify_all(&[SETTLE_NS / 2, mid_session], 0);
    assert_eq!(
        first_session[0].1,
        Cause::Acquisition,
        "a resync inside the first session's settle window is the acquisition transient"
    );
    assert_eq!(
        first_session[1].1,
        Cause::Unexplained,
        "and one nine hours in, with nothing scheduled near it, is not accounted for"
    );
    assert_eq!(first_session[1].1.name(), "UNEXPLAINED");
    assert_eq!(
        count_unexplained(&first_session),
        1,
        "the count the run reports and asserts is zero is the count that sees it"
    );

    // After a modelled restart the settle window is still the only scheduled
    // event, and it is named differently, so the classification distinguishes
    // the two explained cases rather than collapsing them.
    let later_session = classify_all(&[SETTLE_NS / 2, mid_session], 1);
    assert_eq!(later_session[0].1, Cause::AcquisitionAfterRestart);
    assert_eq!(later_session[1].1, Cause::Unexplained);
    assert_eq!(count_unexplained(&later_session), 1);

    // And a run with nothing but scheduled events reports nothing unexplained,
    // which is the assertion the modelled run makes.
    assert_eq!(count_unexplained(&classify_all(&[0, SETTLE_NS], 0)), 0);
}
