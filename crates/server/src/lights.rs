//! The visualizer stream for a control-plane subscriber (`GET
//! /api/visualizer?zone=<room>`, `docs/visualizer.md`, "The HTTP stream").
//!
//! # What it is for
//!
//! A light that follows the music through Home Assistant (K65) is not an
//! endpoint: it holds no audio-wire session, declares no `visualizer` role
//! and has no clock on the server timeline. This module is how the frames
//! the audio thread already computes for a slot reach such a subscriber of
//! the HTTP control plane, at a rate a smart light can take.
//!
//! # One frame per room, superseded and never queued
//!
//! For every room somebody subscribed to, the tap holds ONE frame: the last
//! one the analysis made for the slot that room's group plays, stamped when
//! that room hears it (`crate::conductor::heard_latency_ns`). The audio
//! thread overwrites it ([`LightTap::push`]); nothing is queued, so a
//! subscriber that is slow, or held back by the rate cap, is sent the latest
//! frame and never a backlog. Two things outlive the frame that carried
//! them, because a light needs them and a later frame does not repeat them:
//!
//! - the last beat, with its own stamp (the onset's), until every subscriber
//!   has been sent a frame made at or after it;
//! - the last colour: the analysis makes one at most every 500 ms, and every
//!   frame sent here carries the colour in force.
//!
//! # The rate cap
//!
//! A subscriber is sent at most one frame every [`MIN_FRAME_INTERVAL`]
//! (100 ms: at most 10 a second), measured on the event writer's monotonic
//! clock between the moments it takes two frames for that subscriber. The
//! analysis makes 25 a second, so most frames are superseded; that is the
//! point.
//!
//! # Who touches it
//!
//! - the audio thread: [`LightTap::watching`] (an atomic, no lock) and
//!   [`LightTap::push`] (a short lock, no allocation, and a non-blocking
//!   wake of the event writer), after a tick's broadcast and outside the
//!   grid guard, where it already pushes to the visualizer sessions;
//! - the conductor: [`LightTap::place`], which says which slot each
//!   subscribed room is on and after what latency it hears it;
//! - a control worker: [`LightTap::subscribe`];
//! - the event writer: [`LightSubscription::next`], which renders a frame's
//!   bytes and its `lead_ms` at the moment it is about to write them.
//!
//! No thread is created here. The only clocks are monotonic: the server
//! timeline for the stamps and the lead, `Instant` for the cap.

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::SyncSender;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};

use chorus_audio::MonotonicTimeline;
use chorus_control::json::{self, Value};
use chorus_dsp::visualizer::{Colour, Frame};

/// The least time between two frames sent to one subscriber: at most 10 a
/// second. Cited, not measured: the two smart-light limits
/// `docs/research/research-dsp-phase-b.md` section 2 found are Nanoleaf's
/// external control ("no faster than 10Hz") and Hue's effect rate (under
/// 12.5 Hz), both LEADs; 10 a second is inside both.
pub const MIN_FRAME_INTERVAL: Duration = Duration::from_millis(100);

/// One frame as an HTTP subscriber is sent it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LightFrame {
    /// When the room hears the audio the frame describes, ns on the server
    /// timeline: the frame's own instant, or the onset's when it carries a
    /// beat, plus the room's heard latency.
    pub timestamp_ns: u64,
    /// `timestamp_ns` minus the server timeline's now when the frame was
    /// rendered for writing, ms, rounded down: how long after the server
    /// wrote the frame the room hears it. Negative when the room already
    /// has.
    pub lead_ms: i64,
    /// The held sample peak, 0 to 255 (the wire's `peak`).
    pub peak: u8,
    /// The beat's strength, 0 for none (the wire's `beat`).
    pub beat: u8,
    /// The colour in force (the wire's `color`); all zero before the first.
    pub colour: Colour,
}

/// A frame for room `zone` as one catalog version 2 message, in the canonical
/// encoding (`fixtures/visualizer/http-frame.json`).
pub fn encode(zone: &str, frame: &LightFrame) -> String {
    format!(
        "{{\"v\":2,\"t\":\"visualizer\",\"zone\":{},\"timestamp_ns\":{},\"lead_ms\":{},\
         \"peak\":{},\"beat\":{},\"red\":{},\"green\":{},\"blue\":{},\"brightness\":{},\
         \"transition_ms\":{}}}",
        json::write(&Value::text(zone)),
        frame.timestamp_ns,
        frame.lead_ms,
        frame.peak,
        frame.beat,
        frame.colour.red,
        frame.colour.green,
        frame.colour.blue,
        frame.colour.brightness,
        frame.colour.transition_ms
    )
}

/// The bytes one frame is on an event stream: one `data:` line and the blank
/// line that ends the event (`fixtures/visualizer/http-frame.sse`).
pub fn event(zone: &str, frame: &LightFrame) -> String {
    format!("data: {}\n\n", encode(zone, frame))
}

const NO_COLOUR: Colour = Colour {
    red: 0,
    green: 0,
    blue: 0,
    brightness: 0,
    transition_ms: 0,
};

/// One room somebody subscribed to.
#[derive(Debug)]
struct Room {
    zone: String,
    subscribers: usize,
    /// The slot its group plays on; `None` when it has none (source `none`,
    /// or the one-stream shape, which is never analysed).
    route: Option<usize>,
    heard_latency_ns: u64,
    /// How many frames it has been pushed; a subscriber is sent a frame only
    /// when this passed what it was last sent.
    seq: u64,
    heard_ns: u64,
    peak: u8,
    /// The last beat: the frame that carried it, its strength, its stamp.
    beat: Option<(u64, u8, u64)>,
    colour: Colour,
}

/// The latest frame of every subscribed room.
#[derive(Debug)]
pub struct LightTap {
    rooms: Mutex<Vec<Room>>,
    /// Subscribed rooms on each slot, for the audio thread.
    watchers: Vec<AtomicUsize>,
    timeline: OnceLock<MonotonicTimeline>,
    wake: OnceLock<SyncSender<()>>,
    subscribers: AtomicUsize,
    sent: AtomicU64,
    superseded: AtomicU64,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    match m.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    }
}

impl LightTap {
    /// A tap over `slots` stream slots, with nobody subscribed.
    pub fn new(slots: usize) -> LightTap {
        LightTap {
            rooms: Mutex::new(Vec::new()),
            watchers: (0..slots).map(|_| AtomicUsize::new(0)).collect(),
            timeline: OnceLock::new(),
            wake: OnceLock::new(),
            subscribers: AtomicUsize::new(0),
            sent: AtomicU64::new(0),
            superseded: AtomicU64::new(0),
        }
    }

    /// The server timeline the frames are stamped on, and the way to wake
    /// the event writer when a room has a new one. Called once, at start.
    pub fn connect(&self, timeline: MonotonicTimeline, wake: SyncSender<()>) {
        let _ = self.timeline.set(timeline);
        let _ = self.wake.set(wake);
    }

    /// Whether a subscribed room is on slot `route`, without a lock: the
    /// audio thread analyses a slot somebody watches.
    pub fn watching(&self, route: usize) -> bool {
        self.watchers
            .get(route)
            .is_some_and(|w| w.load(Ordering::Relaxed) > 0)
    }

    /// Subscribers attached now.
    pub fn subscribers(&self) -> usize {
        self.subscribers.load(Ordering::Relaxed)
    }

    /// Frames rendered for a subscriber, over the run.
    pub fn sent(&self) -> u64 {
        self.sent.load(Ordering::Relaxed)
    }

    /// Frames a subscriber was never sent because a later one superseded
    /// them before it could be sent another, over the run.
    pub fn superseded(&self) -> u64 {
        self.superseded.load(Ordering::Relaxed)
    }

    fn recount(&self, rooms: &[Room]) {
        for (slot, watchers) in self.watchers.iter().enumerate() {
            let count = rooms
                .iter()
                .filter(|r| r.subscribers > 0 && r.route == Some(slot))
                .count();
            watchers.store(count, Ordering::Relaxed);
        }
    }

    /// Attach a subscriber to room `zone`: it is sent the frames made from
    /// now on. The conductor's next pass says which slot the room is on.
    pub fn subscribe(self: &Arc<Self>, zone: &str) -> LightSubscription {
        let mut rooms = lock(&self.rooms);
        let at = match rooms.iter().position(|r| r.zone == zone) {
            Some(at) => at,
            None => {
                rooms.push(Room {
                    zone: zone.to_string(),
                    subscribers: 0,
                    route: None,
                    heard_latency_ns: crate::router::DEFAULT_HEARD_LATENCY_NS,
                    seq: 0,
                    heard_ns: 0,
                    peak: 0,
                    beat: None,
                    colour: NO_COLOUR,
                });
                rooms.len() - 1
            }
        };
        rooms[at].subscribers += 1;
        let last_seq = rooms[at].seq;
        self.recount(&rooms);
        self.subscribers.fetch_add(1, Ordering::Relaxed);
        LightSubscription {
            tap: Arc::clone(self),
            zone: zone.to_string(),
            last_seq,
            last_sent: None,
        }
    }

    /// Say where every subscribed room is: `of(room)` is the slot its group
    /// plays on (if it has one) and the latency after which it hears it, ns.
    /// The conductor calls this on every pass.
    pub fn place(&self, mut of: impl FnMut(&str) -> (Option<usize>, u64)) {
        let mut rooms = lock(&self.rooms);
        for room in rooms.iter_mut().filter(|r| r.subscribers > 0) {
            let (route, latency_ns) = of(&room.zone);
            room.route = route.filter(|r| *r < self.watchers.len());
            room.heard_latency_ns = latency_ns;
        }
        self.recount(&rooms);
    }

    /// `frame` describes the audio slot `route` carried at `at_ns` on the
    /// server timeline: it becomes the latest frame of every subscribed room
    /// on that slot, superseding the one before. Called by the audio thread;
    /// allocates nothing and never blocks on a subscriber.
    pub fn push(&self, route: usize, at_ns: u64, frame: &Frame) {
        if !self.watching(route) {
            return;
        }
        let mut any = false;
        {
            let mut rooms = lock(&self.rooms);
            for room in rooms
                .iter_mut()
                .filter(|r| r.subscribers > 0 && r.route == Some(route))
            {
                room.seq += 1;
                room.heard_ns = at_ns.saturating_add(room.heard_latency_ns);
                room.peak = frame.peak;
                if frame.beat > 0 {
                    room.beat = Some((room.seq, frame.beat, room.heard_ns));
                }
                if let Some(colour) = frame.colour {
                    room.colour = colour;
                }
                any = true;
            }
        }
        if any {
            if let Some(wake) = self.wake.get() {
                let _ = wake.try_send(());
            }
        }
    }
}

/// One subscriber of one room's stream, held by the event writer.
#[derive(Debug)]
pub struct LightSubscription {
    tap: Arc<LightTap>,
    zone: String,
    last_seq: u64,
    last_sent: Option<Instant>,
}

impl LightSubscription {
    /// How long until this subscriber may be sent another frame under the
    /// rate cap; `None` when it may be now.
    pub fn wait(&self, now: Instant) -> Option<Duration> {
        let since = now.saturating_duration_since(self.last_sent?);
        (since < MIN_FRAME_INTERVAL).then(|| MIN_FRAME_INTERVAL - since)
    }

    /// The bytes of the next frame to write (`data: <message>` and a blank
    /// line), or `None` when the cap holds it back or the room has no frame
    /// newer than the last one this subscriber was sent.
    ///
    /// The frame is the room's latest. Its beat is the room's last beat when
    /// this subscriber has not been sent a frame made at or after it (a beat
    /// in a superseded frame is carried, stamped at its onset), and 0
    /// otherwise.
    pub fn next(&mut self, now: Instant) -> Option<String> {
        if self.wait(now).is_some() {
            return None;
        }
        let now_ns = self.tap.timeline.get()?.now_ns();
        let frame = {
            let rooms = lock(&self.tap.rooms);
            let room = rooms.iter().find(|r| r.zone == self.zone)?;
            if room.seq == self.last_seq {
                return None;
            }
            let (beat, timestamp_ns) = match room.beat {
                Some((at, strength, heard_ns)) if at > self.last_seq => (strength, heard_ns),
                _ => (0, room.heard_ns),
            };
            self.tap
                .superseded
                .fetch_add(room.seq - self.last_seq - 1, Ordering::Relaxed);
            self.last_seq = room.seq;
            LightFrame {
                timestamp_ns,
                lead_ms: (i128::from(timestamp_ns) - i128::from(now_ns)).div_euclid(1_000_000)
                    as i64,
                peak: room.peak,
                beat,
                colour: room.colour,
            }
        };
        self.last_sent = Some(now);
        self.tap.sent.fetch_add(1, Ordering::Relaxed);
        Some(event(&self.zone, &frame))
    }
}

impl Drop for LightSubscription {
    fn drop(&mut self) {
        let mut rooms = lock(&self.tap.rooms);
        if let Some(room) = rooms.iter_mut().find(|r| r.zone == self.zone) {
            room.subscribers = room.subscribers.saturating_sub(1);
            if room.subscribers == 0 {
                room.route = None;
            }
        }
        self.tap.recount(&rooms);
        self.tap.subscribers.fetch_sub(1, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::mpsc;

    use chorus_dsp::visualizer::{BANDS, FLOOR_DB};

    fn analysed(peak: u8, beat: u8, colour: Option<Colour>) -> Frame {
        Frame {
            at_sample: 0,
            peak,
            beat,
            band_db: [FLOOR_DB as f32; BANDS],
            colour,
        }
    }

    const AMBER: Colour = Colour {
        red: 255,
        green: 96,
        blue: 0,
        brightness: 180,
        transition_ms: 500,
    };

    fn tap() -> (Arc<LightTap>, mpsc::Receiver<()>) {
        let tap = Arc::new(LightTap::new(2));
        let (wake, woken) = mpsc::sync_channel(1);
        tap.connect(MonotonicTimeline::new(), wake);
        (tap, woken)
    }

    fn fixture(name: &str) -> String {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/visualizer")
            .join(name);
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {}", path.display(), e))
    }

    /// `fixtures/visualizer/http-frame.fields` encodes to exactly
    /// `http-frame.json`, and goes onto an event stream as exactly
    /// `http-frame.sse`.
    #[test]
    fn the_committed_frame_is_these_bytes() {
        let fields = fixture("http-frame.fields");
        let field = |key: &str| -> String {
            fields
                .lines()
                .filter(|l| !l.trim_start().starts_with('#'))
                .filter_map(|l| l.split_once('='))
                .find(|(k, _)| k.trim() == key)
                .map(|(_, v)| v.trim().to_string())
                .unwrap_or_else(|| panic!("http-frame.fields has no {}", key))
        };
        assert_eq!(field("message_type"), "visualizer");
        let frame = LightFrame {
            timestamp_ns: field("timestamp_ns").parse().unwrap(),
            lead_ms: field("lead_ms").parse().unwrap(),
            peak: field("peak").parse().unwrap(),
            beat: field("beat").parse().unwrap(),
            colour: Colour {
                red: field("red").parse().unwrap(),
                green: field("green").parse().unwrap(),
                blue: field("blue").parse().unwrap(),
                brightness: field("brightness").parse().unwrap(),
                transition_ms: field("transition_ms").parse().unwrap(),
            },
        };
        let zone = field("zone");
        let json = fixture("http-frame.json");
        assert_eq!(encode(&zone, &frame), json.trim_end());
        assert_eq!(event(&zone, &frame), fixture("http-frame.sse"));
        assert_eq!(
            fixture("http-frame.sse"),
            format!("data: {}\n\n", json.trim_end())
        );
        // It is JSON the catalog's own parser reads back.
        let parsed = json::parse(json.trim_end()).unwrap();
        assert_eq!(parsed.get("t").and_then(Value::as_str), Some("visualizer"));
        assert_eq!(parsed.get("zone").and_then(Value::as_str), Some("den"));
    }

    #[test]
    fn a_slot_is_watched_only_while_a_subscribed_room_is_on_it() {
        let (tap, _woken) = tap();
        assert!(!tap.watching(0) && !tap.watching(1));
        let den = tap.subscribe("den");
        // Subscribed, and not placed yet.
        assert!(!tap.watching(0));
        tap.place(|zone| ((zone == "den").then_some(1), 180_000_000));
        assert!(!tap.watching(0) && tap.watching(1));
        assert_eq!(tap.subscribers(), 1);
        // A slot this server does not have (the silent fanout's index, or
        // the one-stream shape's only fanout) is never watched.
        tap.place(|_| (Some(2), 180_000_000));
        assert!(!tap.watching(0) && !tap.watching(1) && !tap.watching(2));
        tap.place(|_| (Some(0), 180_000_000));
        assert!(tap.watching(0));
        drop(den);
        assert!(!tap.watching(0));
        assert_eq!(tap.subscribers(), 0);
    }

    #[test]
    fn a_subscriber_is_sent_the_latest_frame_and_the_beat_and_colour_it_missed() {
        let (tap, woken) = tap();
        let mut den = tap.subscribe("den");
        let mut study = tap.subscribe("study");
        tap.place(|zone| match zone {
            "den" => (Some(0), 180_000_000),
            _ => (None, 500_000_000),
        });
        let start = Instant::now();
        assert_eq!(den.next(start), None, "nothing was made yet");
        // Three frames before the subscriber is served: a colour, a beat,
        // then a plain one. Only the last is sent, carrying the other two.
        tap.push(0, 1_000_000_000, &analysed(40, 0, Some(AMBER)));
        assert!(woken.try_recv().is_ok(), "the writer was woken");
        tap.push(0, 1_040_000_000, &analysed(200, 255, None));
        tap.push(0, 1_080_000_000, &analysed(90, 0, None));
        // A slot no subscribed room is on is not taken.
        tap.push(1, 9_000_000_000, &analysed(255, 255, None));
        let first = den.next(start).expect("a frame");
        let message = first
            .strip_prefix("data: ")
            .and_then(|m| m.strip_suffix("\n\n"))
            .expect("one data line and a blank line");
        let parsed = json::parse(message).unwrap();
        let int = |key: &str| -> i64 {
            parsed
                .get(key)
                .and_then(Value::as_num)
                .unwrap()
                .parse()
                .unwrap()
        };
        assert_eq!(parsed.get("zone").and_then(Value::as_str), Some("den"));
        // The beat's own stamp (its frame's, plus the heard latency), the
        // latest frame's peak, the colour in force.
        assert_eq!(int("timestamp_ns"), 1_040_000_000 + 180_000_000);
        assert_eq!((int("peak"), int("beat")), (90, 255));
        assert_eq!(
            (int("red"), int("green"), int("blue"), int("brightness")),
            (255, 96, 0, 180)
        );
        assert_eq!(int("transition_ms"), 500);
        // The lead is the stamp against the timeline's now, which started
        // with the tap: a little under 1220 ms.
        assert!((1_000..=1_220).contains(&int("lead_ms")), "{}", message);
        assert_eq!((tap.sent(), tap.superseded()), (1, 2));

        // Nothing new: nothing sent. Something new inside the cap: held.
        assert_eq!(den.next(start + MIN_FRAME_INTERVAL), None);
        tap.push(0, 1_120_000_000, &analysed(80, 0, None));
        assert_eq!(
            den.wait(start + Duration::from_millis(40)),
            Some(Duration::from_millis(60))
        );
        assert_eq!(den.next(start + Duration::from_millis(40)), None);
        tap.push(0, 1_160_000_000, &analysed(70, 0, None));
        assert_eq!(den.wait(start + MIN_FRAME_INTERVAL), None);
        let second = den.next(start + MIN_FRAME_INTERVAL).expect("the latest");
        // The latest, with its own stamp, and the beat is not sent twice.
        assert!(
            second.contains("\"timestamp_ns\":1340000000,") && second.contains("\"beat\":0,"),
            "{}",
            second
        );
        assert!(second.contains("\"peak\":70,"), "{}", second);
        assert_eq!((tap.sent(), tap.superseded()), (2, 3));
        // The room on no slot was sent nothing.
        assert_eq!(study.next(start + Duration::from_secs(5)), None);
    }
}
