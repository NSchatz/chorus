//! A speaker's button press reaches an HTTP subscriber as an event.
//!
//! `GET /api/controller-events` (docs/control-plane.md, "How the messages
//! travel") is a server-sent event stream carrying one `controller_event`
//! message per controller command the server accepted. This drives the real
//! binary: an emulated endpoint with the controller role sends commands in
//! its own session, and subscribers on loopback read what the control plane
//! sends them. What is graded:
//!
//! - one press, exactly one event, with the endpoint's id and the command, in
//!   the bytes `fixtures/control/v2/controller_event.json` pins;
//! - a transport command, which changes no room, is an event all the same;
//! - a subscriber that attaches afterwards is sent no past press;
//! - a command the server refuses is no event;
//! - a press never rides in a state message: the `/api/events` stream beside
//!   it carries states and nothing else;
//! - the MQTT publisher is off for the whole run, so none of this needs it.
//!
//! Nothing here is graded on how long something took. Order is shown by what
//! arrives next on a stream, and the one silence asserted (a late subscriber
//! is sent nothing) is closed by a later press whose event must be the first
//! thing that stream carries.

mod common;

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use chorus_protocol::v2::{roles, Command as Button, ControllerCommand, Message};
use common::{Player, RunningServer};

/// How long a subscriber waits for a message that is on its way. Far longer
/// than it takes; a server that is not going to send it fails in this long.
const DEADLINE: Duration = Duration::from_secs(20);

/// How long a stream is listened to when the claim is that nothing more is on
/// it. Not a timing claim about the server: an event already written is
/// already in the socket by the time the next one this test waited for was.
const QUIET: Duration = Duration::from_millis(300);

/// One server-sent event stream on a raw socket: the subscriber a shell
/// script is, and the protocol a browser's `EventSource` speaks.
struct Stream {
    socket: TcpStream,
    buffered: String,
}

impl Stream {
    /// `GET path`, read as far as the end of the response headers.
    fn open(address: &str, path: &str) -> Stream {
        let mut socket = TcpStream::connect(address).expect("the control plane listens");
        write!(
            socket,
            "GET {} HTTP/1.1\r\nHost: chorus\r\nConnection: close\r\n\r\n",
            path
        )
        .unwrap();
        let mut stream = Stream {
            socket,
            buffered: String::new(),
        };
        let deadline = Instant::now() + DEADLINE;
        while !stream.buffered.contains("\r\n\r\n") {
            assert!(
                stream.fill(deadline),
                "{} never answered its headers: {:?}",
                path,
                stream.buffered
            );
        }
        let (head, rest) = stream.buffered.split_once("\r\n\r\n").unwrap();
        assert!(head.starts_with("HTTP/1.1 200 OK"), "{}: {}", path, head);
        assert!(head.contains("text/event-stream"), "{}: {}", path, head);
        stream.buffered = rest.to_string();
        stream
    }

    /// Read once more, until `deadline`; false when nothing came.
    fn fill(&mut self, deadline: Instant) -> bool {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return false;
        }
        self.socket.set_read_timeout(Some(left)).unwrap();
        let mut scratch = [0u8; 4096];
        match self.socket.read(&mut scratch) {
            Ok(0) | Err(_) => false,
            Ok(n) => {
                self.buffered
                    .push_str(&String::from_utf8_lossy(&scratch[..n]));
                true
            }
        }
    }

    /// The next `data:` message, comments (the opening line, keepalives)
    /// passed over; `None` when none arrives within `limit`.
    fn next(&mut self, limit: Duration) -> Option<String> {
        let deadline = Instant::now() + limit;
        loop {
            while let Some((block, rest)) = self.buffered.split_once("\n\n") {
                let block = block.to_string();
                self.buffered = rest.to_string();
                if let Some(data) = block.strip_prefix("data: ") {
                    return Some(data.to_string());
                }
                assert!(
                    block.starts_with(':'),
                    "an event-stream block that is neither data nor a comment: {:?}",
                    block
                );
            }
            if !self.fill(deadline) {
                return None;
            }
        }
    }

    /// The next message, which has to come.
    fn must(&mut self, what: &str) -> String {
        self.next(DEADLINE)
            .unwrap_or_else(|| panic!("{}: nothing arrived within {:?}", what, DEADLINE))
    }
}

fn press(controller: &mut Player, command: Button, value: i16) {
    press_at(controller, command, value, "");
}

fn press_at(controller: &mut Player, command: Button, value: i16, target: &str) {
    controller
        .session
        .writer
        .send(&Message::ControllerCommand(ControllerCommand {
            command,
            value,
            target: target.to_string(),
        }))
        .unwrap();
}

/// The committed vector, with the fixture's endpoint replaced by this run's.
fn vector(name: &str, endpoint: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/control/v2")
        .join(format!("{}.json", name));
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {}", path.display(), e));
    assert!(text.contains(r#""endpoint":"endpoint-a""#), "{}", text);
    text.trim_end().replace(
        r#""endpoint":"endpoint-a""#,
        &format!(r#""endpoint":"{}""#, endpoint),
    )
}

#[test]
fn a_press_is_one_event_to_an_http_subscriber_and_a_late_subscriber_is_sent_no_past_press() {
    // No `--mqtt-broker`: the events leave by the control plane alone.
    let server = RunningServer::start(&[
        "--source",
        "tone",
        "--serve-forever",
        "--zone",
        "kitchen",
        "--zone",
        "study",
    ]);
    let button = common::fresh_id("http-button");
    server.applied(&format!(
        r#"{{"v":1,"t":"attach","zone":"kitchen","endpoint":"{}"}}"#,
        button
    ));
    server.applied(r#"{"v":1,"t":"volume","zone":"kitchen","volume":0.250}"#);
    let mut controller = Player::connect(&server.audio, &button, roles::CONTROLLER);
    controller.until("the controller's greeting", Duration::from_secs(10), |p| {
        !p.room_volumes().is_empty()
    });

    // Two subscribers of the same control plane: one of the presses, one of
    // the state. Each is attached once its headers are read.
    let mut presses = Stream::open(&server.control, "/api/controller-events");
    let mut states = Stream::open(&server.control, "/api/events");
    let opening = states.must("the opening state");
    assert!(opening.contains(r#""t":"state""#), "{}", opening);

    // One press: volume down, which changes the kitchen.
    press(&mut controller, Button::VolumeStep, -5);
    let event = presses.must("the volume-down event");
    assert_eq!(event, vector("controller_event", &button));
    assert!(event.contains(&format!(r#""endpoint":"{}""#, button)));
    assert!(event.contains(r#""command":"volume_step","value":-5"#));

    // The state it changed goes where states go, as a state.
    let changed = states.must("the kitchen's state after the step");
    assert!(changed.contains(r#""t":"state""#), "{}", changed);
    assert!(changed.contains(r#""volume":0.200"#), "{}", changed);
    assert!(!changed.contains("controller_event"), "{}", changed);
    assert!(!changed.contains("volume_step"), "{}", changed);

    // A second press, a transport command: it changes no room and is still
    // an event. It is the NEXT thing on the stream, so the first press was
    // exactly one event.
    press(&mut controller, Button::Toggle, 0);
    assert_eq!(
        presses.must("the toggle event"),
        vector("controller_event-transport", &button)
    );

    // A subscriber that attaches now has two presses behind it, and is sent
    // neither.
    let mut late = Stream::open(&server.control, "/api/controller-events");
    assert_eq!(
        late.next(QUIET),
        None,
        "a subscriber that attached after two presses was sent a past one"
    );

    // A command the server refuses (a join whose target is not a group name)
    // is no event: what follows it on both streams is the press after it.
    press_at(&mut controller, Button::Join, 0, "Not A Group");

    // A third accepted press reaches both, and it is the first thing the late
    // stream ever carries.
    press(&mut controller, Button::Next, 0);
    let third = presses.must("the next-track event");
    assert!(
        third.contains(r#""command":"next","value":0,"target":"","outcome":"waits-for-an-input""#),
        "{}",
        third
    );
    assert_eq!(late.must("the late subscriber's first event"), third);

    // Three accepted presses, three events, and nothing else on either
    // stream.
    assert_eq!(presses.next(QUIET), None, "more events than presses");
    assert_eq!(late.next(QUIET), None, "more events than presses");
    // The two transport commands changed nothing, so the state stream stayed
    // silent: a press is not a state message.
    assert_eq!(
        states.next(QUIET),
        None,
        "a transport command sent the state subscribers something"
    );

    // Both streams are the event writer's, on the control plane's own count.
    let report = common::http(
        &server.control,
        "GET /api/report HTTP/1.1\r\nHost: chorus\r\nConnection: close\r\n\r\n",
    )
    .1;
    assert!(report.contains("press_subscribers=2"), "{}", report);
    assert!(report.contains("events held=3 "), "{}", report);
    assert!(report.contains("press_dropped_subscribers=0"), "{}", report);
    assert!(report.contains("press_dropped_events=0"), "{}", report);

    // The message is one the server sends, never one it accepts.
    let (status, answer) = server.command(vector("controller_event", &button).as_str());
    assert!(status.contains("400"), "{} {}", status, answer);
}
