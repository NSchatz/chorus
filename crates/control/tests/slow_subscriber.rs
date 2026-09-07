//! AC-11, the subscriber that stops reading.
//!
//! "IF a control subscriber stops reading, or disconnects mid-write THEN THE
//! SYSTEM SHALL drop that subscriber at a bounded queue ceiling, count what it
//! dropped, report the count, and SHALL NOT delay any other subscriber's
//! fanout and SHALL NOT block or reorder the audio path."
//!
//! Four clauses, four assertions. The last one is the one that could be waved
//! at rather than shown, so it is shown: a real `chorus_server::stream::Fanout`
//! carrying real audio items runs beside a control fanout whose subscriber has
//! stalled, and the audio subscriber's sequence is checked for both delay and
//! order.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::TryRecvError;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use chorus_control::fanout::{ControlFanout, CONTROL_QUEUE_LIMIT};

#[test]
fn a_stalled_subscriber_is_dropped_at_the_ceiling_and_what_it_lost_is_counted() {
    let fanout = ControlFanout::new();
    let reading = fanout.subscribe();
    let stalled = fanout.subscribe();

    for index in 0..CONTROL_QUEUE_LIMIT {
        assert_eq!(
            fanout.broadcast(Arc::new(format!("state {}", index))),
            2,
            "at the ceiling and not past it, both are still attached"
        );
        assert!(reading.recv().is_ok());
    }
    assert_eq!(fanout.dropped_subscribers(), 0);

    assert_eq!(
        fanout.broadcast(Arc::new("one too many".to_string())),
        1,
        "past the ceiling the stalled subscriber is gone"
    );
    assert_eq!(fanout.dropped_subscribers(), 1);
    assert_eq!(
        fanout.dropped_messages(),
        CONTROL_QUEUE_LIMIT as u64 + 1,
        "what it never read is counted, not only the one that would not fit"
    );
    assert!(
        fanout.report().contains("dropped_subscribers=1"),
        "the count has to be REPORTED: {}",
        fanout.report()
    );
    assert!(
        fanout
            .report()
            .contains(&format!("dropped_messages={}", CONTROL_QUEUE_LIMIT + 1)),
        "{}",
        fanout.report()
    );

    // Everything the stalled one had queued is still readable from its end;
    // dropping a subscriber closes the sender, it does not empty the queue.
    let mut held = 0;
    loop {
        match stalled.try_recv() {
            Ok(_) => held += 1,
            Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => break,
        }
    }
    assert_eq!(held, CONTROL_QUEUE_LIMIT, "it held exactly its ceiling");
}

#[test]
fn a_subscriber_that_disconnected_mid_write_is_dropped_and_nobody_else_notices() {
    let fanout = ControlFanout::new();
    let reading = fanout.subscribe();
    let gone = fanout.subscribe();
    drop(gone);
    assert_eq!(fanout.broadcast(Arc::new("state".to_string())), 1);
    assert_eq!(fanout.subscribers(), 1);
    assert_eq!(reading.recv().unwrap().as_str(), "state");
}

#[test]
fn a_stalled_subscriber_does_not_delay_another_ones_fanout() {
    let fanout = Arc::new(ControlFanout::new());
    let reading = fanout.subscribe();
    let _stalled = fanout.subscribe();

    // Ten times the ceiling, so the stalled subscriber is filled, dropped, and
    // the rest of the run is served with it gone. Every broadcast is timed.
    let broadcasts = CONTROL_QUEUE_LIMIT * 10;
    let mut worst = Duration::ZERO;
    for index in 0..broadcasts {
        let started = Instant::now();
        fanout.broadcast(Arc::new(format!("state {}", index)));
        worst = worst.max(started.elapsed());
        assert_eq!(
            reading.recv().unwrap().as_str(),
            format!("state {}", index),
            "the reading subscriber gets every message, in order"
        );
    }
    assert!(
        worst < Duration::from_millis(100),
        "the slowest broadcast took {:?}; a stalled subscriber must never make one wait",
        worst
    );
}

#[test]
fn a_stalled_control_subscriber_does_not_block_or_reorder_the_audio_path() {
    use chorus_server::stream::{Fanout, Outbound};

    let audio = Arc::new(Fanout::new());
    let control = Arc::new(ControlFanout::new());
    let (_listening_tx, listening) = audio.subscribe();
    let _stalled_control = control.subscribe();
    let control_reader = control.subscribe();

    // The control fanout is driven hard from its own thread, with one
    // subscriber that never reads, while the audio fanout carries a numbered
    // stream on this one.
    let sent = Arc::new(AtomicU64::new(0));
    let control_thread = {
        let control = Arc::clone(&control);
        let sent = Arc::clone(&sent);
        thread::spawn(move || {
            for index in 0..CONTROL_QUEUE_LIMIT * 20 {
                control.broadcast(Arc::new(format!("state {}", index)));
                sent.fetch_add(1, Ordering::Relaxed);
            }
        })
    };

    // Below the audio fanout's own ceiling, so that this test is about the
    // control fanout's effect on the audio path and never about the audio
    // fanout's own bound, which crates/server's suite already asserts.
    let chunks = chorus_server::stream::SUBSCRIBER_QUEUE_LIMIT - 1;
    let mut worst = Duration::ZERO;
    for index in 0..chunks {
        let frame = Outbound::Frame(Arc::new((index as u32).to_be_bytes().to_vec()));
        let started = Instant::now();
        audio.broadcast(frame);
        worst = worst.max(started.elapsed());
    }
    control_thread.join().expect("the control thread finished");

    // Order: exactly the frames that were sent, in the order they were sent.
    for index in 0..chunks {
        match listening.recv().expect("the audio subscriber has them all") {
            Outbound::Frame(bytes) => assert_eq!(
                bytes.as_slice(),
                &(index as u32).to_be_bytes(),
                "the audio path was reordered"
            ),
            other => panic!("expected an audio frame, got {:?}", other),
        }
    }
    assert!(
        worst < Duration::from_millis(100),
        "the slowest audio broadcast took {:?} with a stalled control subscriber beside it",
        worst
    );
    assert_eq!(audio.dropped(), 0, "no audio was dropped");
    assert!(control.dropped_subscribers() >= 1, "the stalled one did go");
    assert!(
        control_reader.recv().is_ok(),
        "and the control subscriber that was reading is still attached"
    );
    assert_eq!(sent.load(Ordering::Relaxed), (CONTROL_QUEUE_LIMIT * 20) as u64);
}
