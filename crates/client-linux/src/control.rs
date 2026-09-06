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
//! - the zone's name, its group, the address its group's stream is on - are
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
use std::time::Duration;

use chorus_control::catalog::{Volume, VOLUME_SCALE};
use chorus_control::json::{self, Value};

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
}

impl Default for ZoneWatch {
    fn default() -> ZoneWatch {
        ZoneWatch {
            gain: AtomicU32::new(VOLUME_SCALE),
            moves: AtomicU64::new(0),
            updates: AtomicU64::new(0),
            facts: Mutex::new(ZoneFacts::default()),
        }
    }
}

impl ZoneWatch {
    /// A watch that has heard nothing, and therefore plays at full scale.
    pub fn new() -> ZoneWatch {
        ZoneWatch::default()
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
        format!(
            "zone id_known={} name={} group={} audio={} volume={} muted={} gain={} serial={} \
             updates={} moves={}",
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
            self.moves()
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
            let muted = candidate.get("muted").and_then(Value::as_bool).unwrap_or(false);
            let volume = candidate
                .get("volume")
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
            let gain = if muted { Volume::SILENT } else { volume };
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
            match self.open_event_stream() {
                Ok(stream) => {
                    // Say who this is, on every connection and not only the
                    // first. A server that has restarted has the zone's name,
                    // group, volume and mute back out of its state file, and
                    // nothing about which endpoints are switched on: that is a
                    // fact about now and is never read from a file. This is how
                    // it learns it again, and it is the endpoint saying so
                    // rather than anything being said to the endpoint.
                    let _ = self.attach();
                    self.read_events(stream, watch, keep_going)
                }
                Err(_) => {}
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
    ) {
        let mut line = String::new();
        while keep_going() {
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

    const STATE: &str = r#"{"v":1,"t":"state","serial":7,"zones":[{"id":"kitchen","name":"Kitchen","group":"downstairs","volume":0.375,"muted":false,"endpoints":["a"],"present":["a"],"audio":"127.0.0.1:4011"}]}"#;

    #[test]
    fn a_state_message_sets_the_gain_the_audio_path_reads() {
        let watch = ZoneWatch::new();
        assert_eq!(watch.gain(), Volume::FULL, "an endpoint that has heard nothing plays");
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
        for text in ["", "not json", "{}", r#"{"zones":"kitchen"}"#, r#"{"zones":[]}"#] {
            assert!(!watch.absorb(text, "kitchen"), "{}", text);
        }
        assert_eq!(watch.gain().thousandths(), 375, "and nothing moved");
    }
}
