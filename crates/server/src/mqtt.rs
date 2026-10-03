//! The MQTT publisher: one thread that tells a broker what the control plane
//! already knows, and is told nothing back (goal 15; P10, Option M2).
//!
//! # What it is
//!
//! Off unless `--mqtt-broker <host:port>` is given. When on, it publishes the
//! topics of `chorus_mqtt::topic` and nothing else (`docs/mqtt.md` is the
//! guide): a retained `online` on the status topic with a retained `offline`
//! last will, each room's and each saved group's object from the control
//! state message as a retained message, and one message that is not retained
//! for each controller command the server accepted. It never subscribes, has
//! no command topic and publishes nothing under `homeassistant/`: the packets
//! it can send are the four `chorus_mqtt::codec` can encode, and a SUBSCRIBE
//! is not one of them.
//!
//! # One thread, declared like the rest
//!
//! Everything here runs on the `mqtt-publisher` thread, created with the rest
//! of the population before the scheduling report
//! (`crates/server/src/main.rs`) and only when the publisher is on. It is an
//! ordinary thread. Resolving the broker's name, connecting, writing and
//! waiting for an acknowledgement all happen on it and on no other, each
//! under a timeout, so a broker that is down, slow, refusing or silent costs
//! this thread time and nobody else anything.
//!
//! What reaches it never waits for it:
//!
//! - state changes arrive through the control fanout it subscribes to like
//!   any other subscriber (`chorus_control::fanout`); a subscriber that falls
//!   behind is dropped by the fanout, and this one then subscribes again and
//!   reads the state as it stands. Only the latest state matters: a change
//!   superseded before it was published is never published.
//! - controller commands arrive through [`EventTap::offer`], which is a
//!   `try_send` into a bounded queue, called on a client reader thread that
//!   must never block; with the queue full the event is counted and dropped.
//!
//! # What the broker holds, and when it is republished
//!
//! The thread keeps what it believes the broker holds for each retained
//! topic and publishes only what differs, so a volume change republishes one
//! room. A room or saved group that is gone is cleared with a retained empty
//! payload. After every connect that belief is forgotten and everything is
//! published again, because a broker that restarted may hold nothing. Events
//! are never kept for later: one that arrives while the broker is away is
//! dropped, since a button press replayed after a reconnect is worse than
//! one lost.
//!
//! Every message is QoS 1 with one packet in flight: the next is sent when
//! the last was acknowledged, which is also how a dead link is noticed (an
//! acknowledgement that does not come within the response timeout ends the
//! connection). The session is clean, so nothing is ever sent twice
//! (`docs/decisions/0116-an-opt-in-read-only-mqtt-publisher.md` records the
//! choice).
//!
//! Control code: it touches no PCM and is never called from the audio
//! thread. Its only clock is the monotonic one.

use std::collections::BTreeMap;
use std::io::{self, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, SyncSender};
use std::sync::Arc;
use std::time::{Duration, Instant};

use chorus_mqtt::codec::{
    connack_meaning, decode_inbound, Connect, Decoded, Inbound, Publish, Qos, Will, DISCONNECT,
    PINGREQ,
};
use chorus_mqtt::payload::{controller_event, retained_of};
use chorus_mqtt::topic::{Topics, OFFLINE, ONLINE};

use crate::control::ControlState;

/// The Keep Alive asked of the broker when `--mqtt-keepalive-s` is not given,
/// in seconds. A broker declares the client gone, and publishes `offline`,
/// when it has heard nothing for one and a half times this (SPEC 3.1.2.10).
/// ASSUMED: a common default, not a measured choice.
pub const DEFAULT_KEEP_ALIVE_S: u16 = 60;

/// Controller events waiting for the publisher at most. Past it an event is
/// dropped and counted, never waited for.
pub const EVENT_QUEUE_LIMIT: usize = 64;

/// The first wait before trying the broker again; doubled after every
/// attempt that fails, up to [`BACKOFF_CEILING`], and back here once a
/// connection is accepted. ASSUMED values.
const BACKOFF_FIRST: Duration = Duration::from_secs(1);

/// The longest wait between attempts.
const BACKOFF_CEILING: Duration = Duration::from_secs(60);

/// How long one TCP connect may take, per address the broker's name gives.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// How long one write may take.
const WRITE_TIMEOUT: Duration = Duration::from_secs(5);

/// How often the thread looks up when nothing woke it: to notice a stopped
/// run, a controller event, a closed connection and a keep alive that is due.
const POLL: Duration = Duration::from_millis(100);

/// How long a clean stop waits for the broker to acknowledge `offline`.
const FAREWELL_TIMEOUT: Duration = Duration::from_secs(1);

/// How long the supervisor waits for the publisher's goodbye when the run
/// ends, at most (`Farewell`).
pub const FAREWELL_WAIT: Duration = Duration::from_secs(2);

/// What the publisher is configured with.
///
/// No `Debug`: a value of this type holds the password.
#[derive(Clone)]
pub struct Settings {
    /// The broker, `host:port`.
    pub broker: String,
    /// The client identifier.
    pub client_id: String,
    /// The user name, if the broker wants one.
    pub user: Option<String>,
    /// The password, read from `--mqtt-password-file`; never printed.
    pub password: Option<Vec<u8>>,
    /// The topics, under the configured prefix.
    pub topics: Topics,
    /// The Keep Alive, in seconds (at least 1).
    pub keep_alive_s: u16,
}

impl Settings {
    /// The line the server prints about the publisher at start. It says
    /// whether a password is set and never what it is.
    pub fn describe(&self) -> String {
        format!(
            "mqtt publisher broker={} prefix={} client_id={} user={} password={} keepalive_s={} \
             subscribes=never",
            self.broker,
            self.topics.prefix(),
            self.client_id,
            self.user.as_deref().unwrap_or("-"),
            if self.password.is_some() {
                "set"
            } else {
                "none"
            },
            self.keep_alive_s
        )
    }

    /// How long the broker has to answer a CONNECT, a PUBLISH or a PINGREQ:
    /// half the Keep Alive, held between one and ten seconds.
    fn response_timeout(&self) -> Duration {
        Duration::from_millis(u64::from(self.keep_alive_s) * 500)
            .clamp(Duration::from_secs(1), Duration::from_secs(10))
    }

    /// A PINGREQ is sent when nothing was written for this long: half the
    /// Keep Alive, so the broker always hears from the client within it
    /// [MQTT-3.1.2-23].
    fn ping_after(&self) -> Duration {
        Duration::from_millis(u64::from(self.keep_alive_s) * 500)
    }
}

/// Read the password out of `--mqtt-password-file`: the file's bytes, less
/// one trailing line ending. An empty password is refused, because a file
/// that was meant to hold one and holds nothing is a mistake to name.
pub fn read_password(path: &str) -> Result<Vec<u8>, String> {
    let mut bytes = std::fs::read(path).map_err(|e| format!("{}: {}", path, e))?;
    if bytes.last() == Some(&b'\n') {
        bytes.pop();
        if bytes.last() == Some(&b'\r') {
            bytes.pop();
        }
    }
    if bytes.is_empty() {
        return Err(format!("{}: the file holds no password", path));
    }
    if bytes.len() > usize::from(u16::MAX) {
        return Err(format!(
            "{}: {} bytes; an MQTT password is at most 65535",
            path,
            bytes.len()
        ));
    }
    Ok(bytes)
}

/// One accepted controller command, on its way to the publisher.
pub struct ControllerEvent {
    endpoint: String,
    payload: String,
}

/// Where the control plane leaves controller events for the publisher.
pub struct EventTap {
    queue: SyncSender<ControllerEvent>,
    dropped: AtomicU64,
}

impl EventTap {
    /// The tap, and the end the publisher reads.
    pub fn pair() -> (EventTap, Receiver<ControllerEvent>) {
        let (queue, events) = mpsc::sync_channel(EVENT_QUEUE_LIMIT);
        (
            EventTap {
                queue,
                dropped: AtomicU64::new(0),
            },
            events,
        )
    }

    /// Leave one accepted controller command for the publisher. Never blocks:
    /// this runs on a client reader thread. With the queue full, or the
    /// publisher gone, the event is counted and dropped.
    pub fn offer(
        &self,
        endpoint: &str,
        zone: &str,
        command: &str,
        value: i16,
        target: &str,
        outcome: &str,
    ) {
        let event = ControllerEvent {
            endpoint: endpoint.to_string(),
            payload: controller_event(endpoint, zone, command, value, target, outcome),
        };
        if self.queue.try_send(event).is_err() {
            self.dropped.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Events dropped because the queue was full.
    pub fn dropped(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }
}

/// The supervisor's end of a clean stop: dropped when the run ends, it tells
/// the publisher to stop and gives it [`FAREWELL_WAIT`] to publish `offline`
/// and disconnect before the process exits.
pub struct Farewell {
    keep: Arc<AtomicBool>,
    done: Receiver<()>,
}

impl Farewell {
    /// The supervisor's end, and the end [`run`] reports on.
    pub fn pair(keep: Arc<AtomicBool>) -> (Farewell, Sender<()>) {
        let (said, done) = mpsc::channel();
        (Farewell { keep, done }, said)
    }
}

impl Drop for Farewell {
    fn drop(&mut self) {
        self.keep.store(false, Ordering::SeqCst);
        // Ok: the publisher said goodbye (or had nobody to say it to).
        // Disconnected: it is gone. Timeout: the broker did not answer in
        // time, and its will says `offline` for us.
        let _ = self.done.recv_timeout(FAREWELL_WAIT);
    }
}

/// Why a connection ended, or never began.
enum Failure {
    /// The broker's name gave no address, or no address answered.
    Unreachable(String),
    /// The broker answered the CONNECT with a return code other than 0.
    Refused(u8),
    /// The broker sent something a publish-only client is never sent.
    Violation(&'static str),
    /// The broker did not answer in time.
    Silent(&'static str),
    /// The connection closed.
    Closed,
    /// A read or a write failed.
    Io(io::ErrorKind),
    /// A packet this build will not encode (a topic or payload past MQTT's
    /// bounds).
    Encode(String),
}

impl Failure {
    /// One line with no secret in it: none of these holds anything but an
    /// address the operator typed, a code, or the kind of an error.
    fn describe(&self) -> String {
        match self {
            Failure::Unreachable(detail) => format!("unreachable detail=\"{}\"", detail),
            Failure::Refused(code) => format!(
                "refused connack={} detail=\"{}\"",
                code,
                connack_meaning(*code)
            ),
            Failure::Violation(what) => format!("protocol-violation detail=\"{}\"", what),
            Failure::Silent(what) => format!("no-answer waiting_for={}", what),
            Failure::Closed => "closed-by-broker".to_string(),
            Failure::Io(kind) => format!("io detail=\"{:?}\"", kind),
            Failure::Encode(detail) => format!("not-encodable detail=\"{}\"", detail),
        }
    }
}

fn io_failure(e: io::Error) -> Failure {
    Failure::Io(e.kind())
}

/// One connection to the broker.
struct Link {
    stream: TcpStream,
    /// Bytes read and not yet decoded.
    inbound: Vec<u8>,
    /// When the last packet was written, for the keep alive.
    last_write: Instant,
    /// The last packet identifier used; the next is one more, skipping 0.
    packet_id: u16,
    response_timeout: Duration,
}

impl Link {
    /// Connect, send CONNECT, wait for an accepting CONNACK, and publish
    /// `online`.
    fn open(settings: &Settings) -> Result<Link, Failure> {
        let addresses = settings
            .broker
            .to_socket_addrs()
            .map_err(|e| Failure::Unreachable(format!("{}: {}", settings.broker, e.kind())))?;
        let mut last = format!("{}: the name gave no address", settings.broker);
        let mut connected = None;
        for address in addresses {
            match TcpStream::connect_timeout(&address, CONNECT_TIMEOUT) {
                Ok(stream) => {
                    connected = Some(stream);
                    break;
                }
                Err(e) => last = format!("{}: {:?}", settings.broker, e.kind()),
            }
        }
        let stream = connected.ok_or(Failure::Unreachable(last))?;
        stream
            .set_write_timeout(Some(WRITE_TIMEOUT))
            .map_err(io_failure)?;
        // Each packet is small and wants to leave now, not after Nagle's
        // wait for the acknowledgement of the one before.
        let _ = stream.set_nodelay(true);
        let mut link = Link {
            stream,
            inbound: Vec::new(),
            last_write: Instant::now(),
            packet_id: 0,
            response_timeout: settings.response_timeout(),
        };
        let status = settings.topics.status();
        let connect = Connect {
            client_id: &settings.client_id,
            keep_alive_s: settings.keep_alive_s,
            will: Some(Will {
                topic: &status,
                message: OFFLINE.as_bytes(),
                qos: Qos::AtLeastOnce,
                retain: true,
            }),
            user: settings.user.as_deref(),
            password: settings.password.as_deref(),
        }
        .encode()
        .map_err(|e| Failure::Encode(e.to_string()))?;
        link.send(&connect)?;
        // Nothing is published before the broker accepts: a refusing broker
        // discards it anyway [MQTT-3.1.4-5], and `online` must follow the
        // CONNACK so that it lands after any `offline` a takeover of an old
        // connection published.
        match link.next_packet(link.response_timeout, "connack")? {
            Inbound::ConnAck { code: 0, .. } => {}
            Inbound::ConnAck { code, .. } => return Err(Failure::Refused(code)),
            _ => return Err(Failure::Violation("something other than a CONNACK first")),
        }
        link.publish(&status, ONLINE.as_bytes(), true)?;
        Ok(link)
    }

    fn send(&mut self, packet: &[u8]) -> Result<(), Failure> {
        self.stream.write_all(packet).map_err(io_failure)?;
        self.last_write = Instant::now();
        Ok(())
    }

    /// The next whole packet from the broker, within `limit`.
    fn next_packet(&mut self, limit: Duration, what: &'static str) -> Result<Inbound, Failure> {
        let deadline = Instant::now() + limit;
        let mut scratch = [0u8; 64];
        loop {
            match decode_inbound(&self.inbound) {
                Decoded::Packet { packet, used } => {
                    self.inbound.drain(..used);
                    return Ok(packet);
                }
                Decoded::Violation(why) => return Err(Failure::Violation(why)),
                Decoded::Incomplete => {}
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err(Failure::Silent(what));
            }
            self.stream
                .set_read_timeout(Some(left.max(Duration::from_millis(1))))
                .map_err(io_failure)?;
            match self.stream.read(&mut scratch) {
                Ok(0) => return Err(Failure::Closed),
                Ok(n) => self.inbound.extend_from_slice(&scratch[..n]),
                Err(e)
                    if matches!(
                        e.kind(),
                        io::ErrorKind::WouldBlock
                            | io::ErrorKind::TimedOut
                            | io::ErrorKind::Interrupted
                    ) => {}
                Err(e) => return Err(io_failure(e)),
            }
        }
    }

    /// Publish at QoS 1 and wait for its PUBACK.
    fn publish(&mut self, topic: &str, payload: &[u8], retain: bool) -> Result<(), Failure> {
        self.publish_within(topic, payload, retain, self.response_timeout)
    }

    fn publish_within(
        &mut self,
        topic: &str,
        payload: &[u8],
        retain: bool,
        limit: Duration,
    ) -> Result<(), Failure> {
        // A packet identifier is never 0 [MQTT-2.3.1-1]. One packet is in
        // flight at a time, so the one before is always free again.
        self.packet_id = self.packet_id.checked_add(1).unwrap_or(1);
        let packet_id = self.packet_id;
        let packet = Publish {
            topic,
            payload,
            qos: Qos::AtLeastOnce,
            retain,
            packet_id,
        }
        .encode()
        .map_err(|e| Failure::Encode(format!("{}: {}", topic, e)))?;
        self.send(&packet)?;
        match self.next_packet(limit, "puback")? {
            Inbound::PubAck { packet_id: acked } if acked == packet_id => Ok(()),
            Inbound::PubAck { .. } => Err(Failure::Violation(
                "a PUBACK for a packet that is not in flight",
            )),
            _ => Err(Failure::Violation(
                "something other than the PUBACK that was owed",
            )),
        }
    }

    /// Send PINGREQ and wait for the PINGRESP.
    fn ping(&mut self) -> Result<(), Failure> {
        self.send(&PINGREQ)?;
        match self.next_packet(self.response_timeout, "pingresp")? {
            Inbound::PingResp => Ok(()),
            _ => Err(Failure::Violation(
                "something other than the PINGRESP that was owed",
            )),
        }
    }

    /// Look at the socket without waiting: with nothing in flight the broker
    /// owes this client nothing, so anything it sent is a violation, and an
    /// end of stream is the connection closed.
    fn look(&mut self) -> Result<(), Failure> {
        self.stream.set_nonblocking(true).map_err(io_failure)?;
        let mut scratch = [0u8; 16];
        let seen = self.stream.read(&mut scratch);
        self.stream.set_nonblocking(false).map_err(io_failure)?;
        match seen {
            Ok(0) => Err(Failure::Closed),
            Ok(_) => Err(Failure::Violation("a packet nothing was owed for")),
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                ) =>
            {
                Ok(())
            }
            Err(e) => Err(io_failure(e)),
        }
    }

    /// A clean stop: say `offline` (the DISCONNECT that follows makes the
    /// broker discard the will that would have said it, [MQTT-3.14.4-3]),
    /// then DISCONNECT and close. Returns whether `offline` was acknowledged.
    fn farewell(mut self, status: &str) -> bool {
        let said = self
            .publish_within(status, OFFLINE.as_bytes(), true, FAREWELL_TIMEOUT)
            .is_ok();
        // Only a broker that acknowledged `offline` is told the stop was
        // clean. Otherwise the connection is closed with no DISCONNECT, which
        // is what makes the broker publish the will.
        if said {
            let _ = self.send(&DISCONNECT);
        }
        let _ = self.stream.shutdown(std::net::Shutdown::Both);
        said
    }
}

/// The retained topics a state message asks for: topic to payload.
fn wanted_of(topics: &Topics, state: &str) -> Option<BTreeMap<String, String>> {
    let cut = match retained_of(state) {
        Ok(cut) => cut,
        Err(why) => {
            // The server's own encoding; this would be a defect, and the
            // retained topics are left as they are rather than cleared.
            eprintln!("chorus-server: mqtt state not published reason=\"{}\"", why);
            return None;
        }
    };
    let mut wanted = BTreeMap::new();
    for (id, object) in cut.rooms {
        wanted.insert(topics.room(&id), object);
    }
    for (id, object) in cut.groups {
        wanted.insert(topics.group(&id), object);
    }
    Some(wanted)
}

/// Bring the broker to what is wanted, over one live connection: the events
/// first, then every retained topic that differs, then a PINGREQ if the
/// connection has been quiet.
fn serve(
    link: &mut Link,
    settings: &Settings,
    wanted: &BTreeMap<String, String>,
    held: &mut BTreeMap<String, Option<String>>,
    events: &Receiver<ControllerEvent>,
) -> Result<(), Failure> {
    link.look()?;
    while let Ok(event) = events.try_recv() {
        link.publish(
            &settings.topics.speaker_event(&event.endpoint),
            event.payload.as_bytes(),
            false,
        )?;
    }
    for (topic, payload) in wanted {
        if held.get(topic).and_then(|h| h.as_deref()) != Some(payload.as_str()) {
            link.publish(topic, payload.as_bytes(), true)?;
            held.insert(topic.clone(), Some(payload.clone()));
        }
    }
    let gone: Vec<String> = held
        .keys()
        .filter(|topic| !wanted.contains_key(*topic))
        .cloned()
        .collect();
    for topic in gone {
        // A retained message with no payload clears the topic
        // [MQTT-3.3.1-10].
        link.publish(&topic, b"", true)?;
        held.remove(&topic);
    }
    if link.last_write.elapsed() >= settings.ping_after() {
        link.ping()?;
    }
    Ok(())
}

/// The publisher's loop: run on the `mqtt-publisher` thread. Returns when
/// `keep` says stop, after a goodbye to the broker if one is connected, and
/// says so on `done`.
pub fn run(
    settings: Settings,
    state: Arc<ControlState>,
    events: Receiver<ControllerEvent>,
    keep: Arc<AtomicBool>,
    done: Sender<()>,
) {
    // Subscribed before the state is read, so a change between the two is in
    // the queue rather than lost.
    let mut inbox = state.fanout().subscribe();
    let mut wanted = wanted_of(&settings.topics, &state.encoded_state()).unwrap_or_default();
    // What the broker is believed to hold: `Some(payload)` once acknowledged
    // on this connection, `None` for a topic an earlier connection published
    // and this one has not yet.
    let mut held: BTreeMap<String, Option<String>> = BTreeMap::new();
    let mut link: Option<Link> = None;
    let mut backoff = BACKOFF_FIRST;
    let mut next_attempt = Instant::now();
    let mut discarded = 0u64;
    while keep.load(Ordering::SeqCst) {
        if link.is_none() {
            // A press nobody could be told about is not replayed later.
            while events.try_recv().is_ok() {
                discarded += 1;
            }
            if Instant::now() >= next_attempt {
                match Link::open(&settings) {
                    Ok(opened) => {
                        println!(
                            "chorus-server: mqtt connected broker={} client_id={} prefix={}",
                            settings.broker,
                            settings.client_id,
                            settings.topics.prefix()
                        );
                        for payload in held.values_mut() {
                            *payload = None;
                        }
                        backoff = BACKOFF_FIRST;
                        link = Some(opened);
                    }
                    Err(failure) => {
                        eprintln!(
                            "chorus-server: mqtt not-connected broker={} reason={} retry_ms={}",
                            settings.broker,
                            failure.describe(),
                            backoff.as_millis()
                        );
                        next_attempt = Instant::now() + backoff;
                        backoff = (backoff * 2).min(BACKOFF_CEILING);
                    }
                }
            }
        }
        if let Some(live) = link.as_mut() {
            if let Err(failure) = serve(live, &settings, &wanted, &mut held, &events) {
                eprintln!(
                    "chorus-server: mqtt disconnected broker={} reason={} retry_ms={}",
                    settings.broker,
                    failure.describe(),
                    backoff.as_millis()
                );
                link = None;
                next_attempt = Instant::now() + backoff;
                backoff = (backoff * 2).min(BACKOFF_CEILING);
            }
        }
        match inbox.recv_timeout(POLL) {
            Ok(first) => {
                // Latest wins: every message is a whole state.
                let mut latest = first;
                while let Ok(newer) = inbox.try_recv() {
                    latest = newer;
                }
                if let Some(next) = wanted_of(&settings.topics, &latest) {
                    wanted = next;
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            // The fanout dropped this subscriber at its queue's ceiling:
            // attach again, then read the state as it stands.
            Err(RecvTimeoutError::Disconnected) => {
                inbox = state.fanout().subscribe();
                if let Some(next) = wanted_of(&settings.topics, &state.encoded_state()) {
                    wanted = next;
                }
            }
        }
    }
    if let Some(live) = link.take() {
        let said = live.farewell(&settings.topics.status());
        println!(
            "chorus-server: mqtt stopped broker={} offline_acknowledged={} events_discarded={}",
            settings.broker,
            u8::from(said),
            discarded
        );
    }
    let _ = done.send(());
}

#[cfg(test)]
mod tests {
    use super::*;

    use chorus_control::rooms::{NowPlaying, PlayState, Source};
    use chorus_control::zones::{Zone, Zones};

    /// (goal 16) What a room is playing reaches its retained topic with no
    /// change to the publisher: the room topic's payload is the control
    /// state's room object, and the room object is where the model puts
    /// `source` and `now_playing`. A change of what plays changes the topics
    /// of the rooms that play it and no other.
    #[test]
    fn a_rooms_retained_payload_carries_what_it_is_playing_when_the_state_does() {
        let topics = Topics::new("chorus/v1").unwrap();
        let mut zones = Zones::new("127.0.0.1:4010");
        for id in ["kitchen", "study"] {
            zones.add(Zone::new(id)).unwrap();
        }
        let before = wanted_of(&topics, &zones.encode_state()).unwrap();
        let kitchen = topics.room("kitchen");
        let study = topics.room("study");
        assert!(!before[&kitchen].contains("now_playing"));

        zones
            .set_group_source("kitchen", Source::Player("p0".to_string()))
            .unwrap();
        zones
            .set_now_playing(
                "kitchen",
                Some(NowPlaying {
                    title: Some("So What".to_string()),
                    artist: Some("Miles Davis".to_string()),
                    album: None,
                    art_url: None,
                    duration_ms: Some(215_000),
                    state: PlayState::Playing,
                    via: "upnp".to_string(),
                }),
            )
            .unwrap();
        let playing = wanted_of(&topics, &zones.encode_state()).unwrap();
        assert!(
            playing[&kitchen].ends_with(concat!(
                r#","source":"player:p0","now_playing":{"title":"So What","#,
                r#""artist":"Miles Davis","album":null,"art_url":null,"#,
                r#""duration_ms":215000,"state":"playing","via":"upnp"}}"#
            )),
            "{}",
            playing[&kitchen]
        );
        // The payload is still the state's own bytes for that room.
        assert!(zones.encode_state().contains(playing[&kitchen].as_str()));
        // Only the room that plays it is published again.
        assert_ne!(playing[&kitchen], before[&kitchen]);
        assert_eq!(playing[&study], before[&study]);

        // Stopped: the room's payload is what it was before anything played.
        zones.set_group_source("kitchen", Source::Stream).unwrap();
        let after = wanted_of(&topics, &zones.encode_state()).unwrap();
        assert_eq!(after[&kitchen], before[&kitchen]);
    }
}
