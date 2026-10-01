//! The endpoint's side of the control channel.
//!
//! An endpoint subscribes to the server's state, keeps the part of it that is
//! about its own zone, and applies that to the audio it is playing. It sends
//! exactly one command of its own - `attach`, which says that this endpoint is
//! playing this zone - and otherwise only listens. Everything else is the
//! server's to decide, which is what "server-authoritative" means and what
//! makes two endpoints in one zone agree without talking to each other.
//!
//! # What the audio path reads
//!
//! One atomic. [`ZoneWatch::gain`] is the whole of what
//! `crates/client-linux/src/run.rs` takes from here per chunk, and it is a
//! `u32` load with no lock, no allocation and no parsing behind it. The strings
//! (the zone's name, its group, the address its group's stream is on) are
//! behind a mutex and are read by the SESSION SUPERVISOR between sessions,
//! never by the loop that is writing to a DAC.
//!
//! # A control channel that is not there is not an error
//!
//! An endpoint started with no `--control` plays at full scale, which is what
//! it did before this phase existed. An endpoint whose control channel drops
//! keeps playing at whatever gain it last knew, and reconnects: the last thing
//! a person wants when the control plane restarts is silence in every room.
//! The gain is held rather than reset for the same reason the sync loop holds
//! its correction when the offset goes stale.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use chorus_control::catalog::{Volume, VOLUME_SCALE};
use chorus_control::json::{self, Value};
use chorus_protocol::v2::{RoomVolume, Sound};

use crate::zone::{RoomGain, RoomVolumeInbox, SoundInbox, ZoneGain};

/// How long the endpoint waits for the control channel before giving up on one
/// attempt.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);

/// How long it waits before trying a dropped control channel again.
pub const RETRY_INTERVAL: Duration = Duration::from_millis(500);

/// The strings from the state message that are about this endpoint's zone.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ZoneFacts {
    /// Whether a state message naming this zone has been seen at all.
    pub known: bool,
    /// The zone's human-set name.
    pub name: String,
    /// The group whose stream this zone plays.
    pub group: String,
    /// Where that group's stream is served.
    pub audio: String,
    /// Whether the zone is muted.
    pub muted: bool,
    /// The zone's volume, in thousandths, whatever the mute is doing.
    pub volume_thousandths: u32,
    /// The serial of the state message this came from.
    pub serial: u64,
}

/// What the endpoint knows about its own zone right now.
#[derive(Debug)]
pub struct ZoneWatch {
    /// The amplitude factor the audio path multiplies by, in thousandths.
    gain: AtomicU32,
    /// Bumped every time the address this endpoint should be playing from
    /// changes, which is how the session supervisor learns to move.
    moves: AtomicU64,
    /// State messages applied.
    updates: AtomicU64,
    facts: Mutex<ZoneFacts>,
    /// The room's volume from the audio wire (`room_volume`, goal 11): where
    /// the session delivers it, and the gain, limit, ramp and ceiling the
    /// playout loop applies. Held for the life of the process, like the zone
    /// gain above: a new session is not a reason to play louder.
    room_inbox: Arc<RoomVolumeInbox>,
    room: Mutex<RoomGain>,
    /// The room's sound from the audio wire (`sound`, goal 12), kept.
    sound_inbox: Arc<SoundInbox>,
}

impl Default for ZoneWatch {
    fn default() -> ZoneWatch {
        ZoneWatch::with_max_volume(Volume::FULL)
    }
}

impl ZoneWatch {
    /// A watch that has heard nothing, and therefore plays at full scale.
    pub fn new() -> ZoneWatch {
        ZoneWatch::default()
    }

    /// A watch with this endpoint's own volume ceiling (`--max-volume`): it
    /// plays at the ceiling until something lowers it, and nothing raises it.
    pub fn with_max_volume(ceiling: Volume) -> ZoneWatch {
        ZoneWatch {
            gain: AtomicU32::new(VOLUME_SCALE),
            moves: AtomicU64::new(0),
            updates: AtomicU64::new(0),
            facts: Mutex::new(ZoneFacts::default()),
            room_inbox: Arc::new(RoomVolumeInbox::default()),
            room: Mutex::new(RoomGain::new(ceiling)),
            sound_inbox: Arc::new(SoundInbox::default()),
        }
    }

    /// Where the audio session delivers `sound`
    /// (`crate::session::deliver_sound_to`).
    pub fn sound_inbox(&self) -> Arc<SoundInbox> {
        Arc::clone(&self.sound_inbox)
    }

    /// A `sound` received since the last call, for the playout loop to log.
    pub fn take_sound(&self) -> Option<Sound> {
        self.sound_inbox.take()
    }

    /// The last `sound` received this process, which the endpoint DSP
    /// configures its chain from; `None` until the server sends one.
    pub fn last_sound(&self) -> Option<Sound> {
        self.sound_inbox.last()
    }

    /// Where the audio session delivers `room_volume`
    /// (`crate::session::deliver_room_volume_to`).
    pub fn room_inbox(&self) -> Arc<RoomVolumeInbox> {
        Arc::clone(&self.room_inbox)
    }

    /// The room's state, copied out, for a status line and the tests.
    pub fn room(&self) -> RoomGain {
        match self.room.lock() {
            Ok(g) => g.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        }
    }

    /// Apply everything this endpoint knows about its volume to `pcm`, the
    /// next frames to be written: min(the room's ramped gain, the room's
    /// limit, this endpoint's ceiling, the control plane's zone gain), frame
    /// by frame, never changing how many frames there are. A `room_volume`
    /// waiting in the inbox is taken first and returned, so the caller can
    /// log it. Called by the playout loop only, so the lock is uncontended.
    pub fn apply(
        &self,
        applier: &ZoneGain,
        channels: usize,
        rate_hz: u32,
        pcm: &mut [u8],
    ) -> Option<RoomVolume> {
        let zone = self.gain();
        let mut room = match self.room.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        let taken = self.room_inbox.take();
        if let Some(m) = &taken {
            room.set(m, zone, rate_hz);
        }
        room.apply(zone, applier, channels, pcm);
        taken
    }

    /// The gain the audio path multiplies by. One atomic load.
    pub fn gain(&self) -> Volume {
        Volume::from_thousandths(i64::from(self.gain.load(Ordering::Relaxed)))
            .unwrap_or(Volume::FULL)
    }

    /// How many times the address this endpoint should play from has changed.
    pub fn moves(&self) -> u64 {
        self.moves.load(Ordering::Relaxed)
    }

    /// How many state messages have been applied.
    pub fn updates(&self) -> u64 {
        self.updates.load(Ordering::Relaxed)
    }

    /// The strings, copied out.
    pub fn facts(&self) -> ZoneFacts {
        match self.facts.lock() {
            Ok(g) => g.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        }
    }

    /// One line for a status report.
    pub fn line(&self) -> String {
        let facts = self.facts();
        let room = self.room();
        format!(
            "zone id_known={} name={} group={} audio={} volume={} muted={} gain={} serial={} \
             updates={} moves={} room_limit={} max_volume={} room_volumes={}",
            u8::from(facts.known),
            facts.name,
            facts.group,
            facts.audio,
            Volume::from_thousandths(i64::from(facts.volume_thousandths))
                .unwrap_or(Volume::FULL)
                .literal(),
            u8::from(facts.muted),
            self.gain().literal(),
            facts.serial,
            self.updates(),
            self.moves(),
            room.limit().literal(),
            room.ceiling().literal(),
            self.room_inbox.received()
        )
    }

    /// Take in one state message and keep the part about `zone`.
    ///
    /// Returns whether the message named this zone at all. A state message that
    /// does not name it changes nothing: an endpoint whose zone has been
    /// removed from the server's configuration keeps playing at the gain it
    /// last knew rather than jumping to full scale.
    pub fn absorb(&self, state: &str, zone: &str) -> bool {
        let value = match json::parse(state) {
            Ok(value) => value,
            Err(_) => return false,
        };
        let zones = match value.get("zones") {
            Some(Value::Arr(zones)) => zones,
            _ => return false,
        };
        let serial = value
            .get("serial")
            .and_then(Value::as_num)
            .and_then(|n| n.parse::<u64>().ok())
            .unwrap_or(0);
        for candidate in zones {
            if candidate.get("id").and_then(Value::as_str) != Some(zone) {
                continue;
            }
            let muted = candidate
                .get("muted")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let volume = candidate
                .get("volume")
                .and_then(Value::as_num)
                .and_then(Volume::parse)
                .unwrap_or(Volume::FULL);
            // A state that also carries the room's effective limit (the v2
            // state, goal 11) bounds the gain by it as well: the server has
            // already clamped `volume` to it, and holding it here too is the
            // endpoint enforcing it rather than trusting that (I10).
            let limit = candidate
                .get("effective_limit")
                .and_then(Value::as_num)
                .and_then(Volume::parse)
                .unwrap_or(Volume::FULL);
            let facts = ZoneFacts {
                known: true,
                name: candidate
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or(zone)
                    .to_string(),
                group: candidate
                    .get("group")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                audio: candidate
                    .get("audio")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                muted,
                volume_thousandths: volume.thousandths(),
                serial,
            };
            let gain = if muted {
                Volume::SILENT
            } else if limit.thousandths() < volume.thousandths() {
                limit
            } else {
                volume
            };
            let moved = {
                let mut held = match self.facts.lock() {
                    Ok(g) => g,
                    Err(poisoned) => poisoned.into_inner(),
                };
                let moved = held.known && held.audio != facts.audio;
                *held = facts;
                moved
            };
            // The gain is stored AFTER the strings, so a reader that saw the
            // new address has certainly seen the new gain too.
            self.gain.store(gain.thousandths(), Ordering::Relaxed);
            self.updates.fetch_add(1, Ordering::Relaxed);
            if moved {
                self.moves.fetch_add(1, Ordering::Relaxed);
            }
            return true;
        }
        false
    }
}

/// The endpoint's connection to one control channel.
#[derive(Debug, Clone)]
pub struct ControlLink {
    /// The control channel's address.
    pub address: String,
    /// The zone this endpoint plays.
    pub zone: String,
    /// This endpoint's identifier.
    pub endpoint: String,
}

impl ControlLink {
    /// Tell the server this endpoint is playing this zone.
    pub fn attach(&self) -> std::io::Result<String> {
        self.post(
            "/api/command",
            &format!(
                r#"{{"v":1,"t":"attach","zone":"{}","endpoint":"{}"}}"#,
                self.zone, self.endpoint
            ),
        )
    }

    /// Tell the server this endpoint has gone.
    pub fn leaving(&self) -> std::io::Result<String> {
        self.post("/api/leaving", &self.endpoint)
    }

    /// The state as it stands, in one request.
    pub fn state(&self) -> std::io::Result<String> {
        let mut connection = self.connect()?;
        write!(
            connection,
            "GET /api/state HTTP/1.1\r\nHost: chorus\r\nConnection: close\r\n\r\n"
        )?;
        connection.flush()?;
        let mut body = String::new();
        connection.read_to_string(&mut body)?;
        Ok(body
            .split_once("\r\n\r\n")
            .map(|(_, body)| body.to_string())
            .unwrap_or(body))
    }

    /// Follow the state, applying every message to `watch`, until `keep_going`
    /// says to stop.
    ///
    /// Reconnects by itself. A control channel that is not there yet, or that
    /// has gone away, is a thing to keep trying: the endpoint has audio to play
    /// meanwhile and this must never be what stops it.
    pub fn follow(&self, watch: &Arc<ZoneWatch>, keep_going: &dyn Fn() -> bool) {
        while keep_going() {
            if let Ok(stream) = self.open_event_stream() {
                // Say who this is, on every connection and not only the
                // first. A server that has restarted has the zone's name,
                // group, volume and mute back out of its state file, and
                // nothing about which endpoints are switched on: that is a
                // fact about now and is never read from a file. This is how
                // it learns it again, and it is the endpoint saying so
                // rather than anything being said to the endpoint.
                //
                // An attach that is turned away (every control worker busy,
                // `503`, when a whole house starts at once) is tried again
                // while the stream is held, every RETRY_INTERVAL, until one is
                // applied: an endpoint the server never heard attach is in no
                // room, so in the slot shape it hears silence, and it is never
                // sent its room's `room_volume`. Found by the house soak
                // (docs/decisions/0078-the-house-soak.md).
                let attached = self.attach().is_ok();
                self.read_events(stream, watch, keep_going, attached)
            }
            let mut waited = Duration::ZERO;
            while keep_going() && waited < RETRY_INTERVAL {
                std::thread::sleep(Duration::from_millis(50));
                waited += Duration::from_millis(50);
            }
        }
    }

    fn connect(&self) -> std::io::Result<TcpStream> {
        let mut last = None;
        for address in std::net::ToSocketAddrs::to_socket_addrs(&self.address)? {
            match TcpStream::connect_timeout(&address, CONNECT_TIMEOUT) {
                Ok(connection) => {
                    let _ = connection.set_nodelay(true);
                    return Ok(connection);
                }
                Err(e) => last = Some(e),
            }
        }
        Err(last.unwrap_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::AddrNotAvailable,
                format!("'{}' resolved to no address", self.address),
            )
        }))
    }

    fn post(&self, path: &str, body: &str) -> std::io::Result<String> {
        let mut connection = self.connect()?;
        connection.set_read_timeout(Some(CONNECT_TIMEOUT))?;
        write!(
            connection,
            "POST {} HTTP/1.1\r\nHost: chorus\r\nContent-Type: application/json\r\n\
             Content-Length: {}\r\nConnection: close\r\n\r\n{}",
            path,
            body.len(),
            body
        )?;
        connection.flush()?;
        let mut response = String::new();
        connection.read_to_string(&mut response)?;
        let status = response.lines().next().unwrap_or_default().to_string();
        if !status.contains(" 200 ") {
            return Err(std::io::Error::other(format!(
                "the control channel answered '{}' to {}",
                status.trim(),
                path
            )));
        }
        Ok(response
            .split_once("\r\n\r\n")
            .map(|(_, body)| body.to_string())
            .unwrap_or(response))
    }

    fn open_event_stream(&self) -> std::io::Result<BufReader<TcpStream>> {
        let mut connection = self.connect()?;
        // Long enough that an idle stream is not mistaken for a dead one, and
        // short enough that a stopping endpoint does not have to wait for it.
        connection.set_read_timeout(Some(Duration::from_millis(250)))?;
        write!(
            connection,
            "GET /api/events HTTP/1.1\r\nHost: chorus\r\nAccept: text/event-stream\r\n\
             Connection: close\r\n\r\n"
        )?;
        connection.flush()?;
        Ok(BufReader::new(connection))
    }

    fn read_events(
        &self,
        mut stream: BufReader<TcpStream>,
        watch: &Arc<ZoneWatch>,
        keep_going: &dyn Fn() -> bool,
        mut attached: bool,
    ) {
        let mut line = String::new();
        let mut last_attach = Instant::now();
        while keep_going() {
            if !attached && last_attach.elapsed() >= RETRY_INTERVAL {
                attached = self.attach().is_ok();
                last_attach = Instant::now();
            }
            line.clear();
            match stream.read_line(&mut line) {
                Ok(0) => return,
                Ok(_) => {}
                Err(ref e)
                    if e.kind() == std::io::ErrorKind::WouldBlock
                        || e.kind() == std::io::ErrorKind::TimedOut =>
                {
                    continue
                }
                Err(_) => return,
            }
            if let Some(payload) = line.trim_end().strip_prefix("data: ") {
                watch.absorb(payload, &self.zone);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A control channel that turns the first attach away with `503`, as a
    /// server whose workers are all busy does, and applies the next one; its
    /// event stream stays open and quiet. The endpoint must try again while it
    /// holds the stream (the house soak's finding, ADR 0078).
    #[test]
    fn an_attach_turned_away_is_tried_again_while_the_stream_is_held() {
        use std::net::TcpListener;
        use std::sync::atomic::AtomicBool;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap().to_string();
        let attaches = Arc::new(AtomicU32::new(0));
        let applied = Arc::new(AtomicBool::new(false));
        {
            let attaches = Arc::clone(&attaches);
            let applied = Arc::clone(&applied);
            std::thread::spawn(move || {
                let mut held = Vec::new();
                for connection in listener.incoming() {
                    let mut connection = connection.unwrap();
                    // Read until the request is whole: the client writes its
                    // headers and its body in more than one segment.
                    let mut request = String::new();
                    let mut buf = [0u8; 4096];
                    while !(request.starts_with("GET") && request.contains("\r\n\r\n"))
                        && !request.ends_with('}')
                    {
                        match connection.read(&mut buf) {
                            Ok(0) | Err(_) => break,
                            Ok(n) => request.push_str(&String::from_utf8_lossy(&buf[..n])),
                        }
                    }
                    if request.starts_with("GET /api/events") {
                        let _ = connection.write_all(
                            b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\n\r\n",
                        );
                        held.push(connection);
                    } else if request.contains(r#""t":"attach""#) {
                        if attaches.fetch_add(1, Ordering::SeqCst) == 0 {
                            let _ = connection.write_all(
                                b"HTTP/1.1 503 Service Unavailable\r\nConnection: close\r\n\r\n",
                            );
                        } else {
                            let _ = connection
                                .write_all(b"HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n{}");
                            applied.store(true, Ordering::SeqCst);
                        }
                    }
                }
            });
        }
        let link = ControlLink {
            address,
            zone: "kitchen".to_string(),
            endpoint: "endpoint-a".to_string(),
        };
        let watch = Arc::new(ZoneWatch::new());
        let stop = Arc::new(AtomicBool::new(false));
        let follower = {
            let stop = Arc::clone(&stop);
            std::thread::spawn(move || link.follow(&watch, &|| !stop.load(Ordering::SeqCst)))
        };
        let deadline = Instant::now() + Duration::from_secs(10);
        while !applied.load(Ordering::SeqCst) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        stop.store(true, Ordering::SeqCst);
        follower.join().unwrap();
        assert!(
            applied.load(Ordering::SeqCst),
            "the attach was tried again and applied ({} attempts)",
            attaches.load(Ordering::SeqCst)
        );
        assert_eq!(
            attaches.load(Ordering::SeqCst),
            2,
            "and not again once it was applied, on the one stream"
        );
    }

    const STATE: &str = r#"{"v":1,"t":"state","serial":7,"zones":[{"id":"kitchen","name":"Kitchen","group":"downstairs","volume":0.375,"muted":false,"endpoints":["a"],"present":["a"],"audio":"127.0.0.1:4011"}]}"#;

    #[test]
    fn a_state_message_sets_the_gain_the_audio_path_reads() {
        let watch = ZoneWatch::new();
        assert_eq!(
            watch.gain(),
            Volume::FULL,
            "an endpoint that has heard nothing plays"
        );
        assert!(watch.absorb(STATE, "kitchen"));
        assert_eq!(watch.gain().thousandths(), 375);
        assert_eq!(watch.facts().audio, "127.0.0.1:4011");
        assert_eq!(watch.facts().name, "Kitchen");
    }

    #[test]
    fn a_mute_takes_the_gain_to_zero_and_keeps_the_volume() {
        let watch = ZoneWatch::new();
        watch.absorb(STATE, "kitchen");
        let muted = STATE.replace(r#""muted":false"#, r#""muted":true"#);
        assert!(watch.absorb(&muted, "kitchen"));
        assert_eq!(watch.gain(), Volume::SILENT);
        assert_eq!(watch.facts().volume_thousandths, 375);
        assert!(watch.facts().muted);
    }

    #[test]
    fn a_state_message_about_another_zone_changes_nothing() {
        let watch = ZoneWatch::new();
        watch.absorb(STATE, "kitchen");
        assert!(!watch.absorb(STATE, "study"));
        assert_eq!(watch.gain().thousandths(), 375, "and the gain is held");
    }

    #[test]
    fn a_change_of_stream_address_is_counted_as_a_move_and_the_first_one_is_not() {
        let watch = ZoneWatch::new();
        watch.absorb(STATE, "kitchen");
        assert_eq!(watch.moves(), 0, "learning where to play is not moving");
        let moved = STATE.replace("127.0.0.1:4011", "127.0.0.1:4012");
        watch.absorb(&moved, "kitchen");
        assert_eq!(watch.moves(), 1);
        watch.absorb(&moved, "kitchen");
        assert_eq!(watch.moves(), 1, "the same address again is not a move");
    }

    #[test]
    fn a_state_message_that_is_not_one_is_ignored_rather_than_acted_on() {
        let watch = ZoneWatch::new();
        watch.absorb(STATE, "kitchen");
        for text in [
            "",
            "not json",
            "{}",
            r#"{"zones":"kitchen"}"#,
            r#"{"zones":[]}"#,
        ] {
            assert!(!watch.absorb(text, "kitchen"), "{}", text);
        }
        assert_eq!(watch.gain().thousandths(), 375, "and nothing moved");
    }

    #[test]
    fn a_v2_state_with_an_effective_limit_bounds_the_gain_by_it() {
        let watch = ZoneWatch::new();
        let v2 = STATE.replace(
            r#""muted":false"#,
            r#""muted":false,"effective_limit":0.250"#,
        );
        assert!(watch.absorb(&v2, "kitchen"));
        assert_eq!(watch.gain().thousandths(), 250, "min(0.375, 0.250)");
        let v2 = STATE.replace(
            r#""muted":false"#,
            r#""muted":false,"effective_limit":0.900"#,
        );
        assert!(watch.absorb(&v2, "kitchen"));
        assert_eq!(
            watch.gain().thousandths(),
            375,
            "a limit above the volume changes nothing"
        );
    }
}
