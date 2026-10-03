//! The supervisor protocol: what `chorus-soloistd` and chorus-server say to
//! each other on a receiver's Unix socket (`r<i>.sock`).
//!
//! Newline-delimited JSON objects, UTF-8, at most [`MAX_LINE`] bytes a line
//! (the line feed included), both ways. Every message has a `"t"` naming its
//! kind. `docs/soloist.md` is the reference; `fixtures/soloist/protocol-*`
//! pins every message byte for byte.
//!
//! - The supervisor speaks [`FromSupervisor`]: `hello` first on every
//!   connection, then `build`, then `status`, then `status`, `build` and
//!   `event` as things change.
//! - The server speaks [`ToSupervisor`]: `assign`, `release`, `command`,
//!   `restart`.
//!
//! A `generation` is the server's counter: `assign` sets it, and the
//! supervisor echoes it in every `status` and `event`, so the server can
//! drop what belongs to a target it has since replaced.
//!
//! A kind this version does not know decodes as
//! [`ProtocolError::UnknownKind`]; a reader skips it, so either side can gain
//! a message without breaking the other.

use chorus_control::json::{self, Value};

/// The protocol version `hello` carries.
pub const VERSION: u32 = 1;

/// The longest line either side sends or accepts, in bytes, its line feed
/// included.
pub const MAX_LINE: usize = 64 * 1024;

/// The longest `version` text a `build` message carries, in characters.
pub const MAX_VERSION_CHARS: usize = 200;

/// What a receiver is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// No target assigned; Soloist is not running.
    Idle,
    /// A target is assigned and Soloist is being started, or its WebSocket
    /// is not connected yet.
    Starting,
    /// Soloist is up and its WebSocket is connected.
    Running,
    /// Soloist exited with code 10: the build has expired. Nothing restarts
    /// it until a `restart`.
    Expired,
    /// Soloist keeps failing; the supervisor retries with a capped backoff
    /// and says so in `detail`.
    Failed,
    /// There is no executable at the supervisor's `--soloist-bin`.
    NoBinary,
}

impl State {
    /// The state as the wire spells it.
    pub fn as_str(self) -> &'static str {
        match self {
            State::Idle => "idle",
            State::Starting => "starting",
            State::Running => "running",
            State::Expired => "expired",
            State::Failed => "failed",
            State::NoBinary => "no-binary",
        }
    }

    /// The state a wire spelling names.
    pub fn from_wire(text: &str) -> Option<State> {
        Some(match text {
            "idle" => State::Idle,
            "starting" => State::Starting,
            "running" => State::Running,
            "expired" => State::Expired,
            "failed" => State::Failed,
            "no-binary" => State::NoBinary,
            _ => return None,
        })
    }
}

/// The `build` message: what `soloist --version` said and when the build
/// expires.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildReport {
    /// Whether an executable exists at `--soloist-bin`.
    pub present: bool,
    /// The first line of `soloist --version`, at most
    /// [`MAX_VERSION_CHARS`] characters; empty when absent.
    pub version: String,
    /// The build's time in Unix seconds, when the version output named one.
    pub build_epoch: Option<u64>,
    /// `build_epoch` plus the documented 90-day lifetime. `None` is "expiry
    /// unknown", never "expired".
    pub expires_epoch: Option<u64>,
}

/// The `status` message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusReport {
    /// What the receiver is doing.
    pub state: State,
    /// The assigned target's key, or empty.
    pub target: String,
    /// The Spotify Connect device name Soloist runs with, or empty.
    pub name: String,
    /// A short reason, or empty.
    pub detail: String,
    /// The generation of the assignment this status belongs to (0 before
    /// any).
    pub generation: u64,
}

/// A message from the supervisor to the server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FromSupervisor {
    /// First on every connection.
    Hello {
        /// The protocol version, [`VERSION`].
        v: u32,
        /// The receiver's index.
        receiver: usize,
        /// The supervisor's own version.
        supervisor: String,
    },
    /// After `hello` and whenever it changes.
    Build(BuildReport),
    /// After `hello` and on every change.
    Status(StatusReport),
    /// One JSON text frame Soloist sent, as it sent it (the API key redacted
    /// by value). Parse `event` with [`crate::api::event_from_value`].
    Event {
        /// The generation of the assignment that was running.
        generation: u64,
        /// Soloist's event object.
        event: Value,
    },
}

/// A message from the server to the supervisor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToSupervisor {
    /// Run Soloist for this target under this device name. A different
    /// target or name than the running one means stop, then start.
    Assign {
        /// The server's counter, echoed in `status` and `event`.
        generation: u64,
        /// The target's key (`room:<id>`, `group:<id>`, `live:<a>+<b>`).
        target: String,
        /// The Spotify Connect device name.
        name: String,
    },
    /// Stop Soloist and go idle.
    Release {
        /// The server's counter.
        generation: u64,
    },
    /// A Soloist WebSocket command object, sent as is. Dropped with a
    /// `status` detail when Soloist is not `running`. Build one with
    /// [`crate::api::Command::to_value`].
    Command {
        /// The generation the command is meant for; a stale one is dropped.
        generation: u64,
        /// The command object.
        command: Value,
    },
    /// Re-read the binary's version, clear `expired` or `failed`, start
    /// again.
    Restart,
}

/// Why a line is not a message, or a message is not a line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProtocolError {
    /// The line is longer than [`MAX_LINE`].
    TooLong,
    /// The line is not UTF-8.
    NotUtf8,
    /// Not JSON the workspace's reader accepts.
    Json(String),
    /// JSON without a string `"t"`.
    NoKind,
    /// A `"t"` this version does not know; skip the line.
    UnknownKind(String),
    /// A known kind with a member missing or of the wrong type; carries
    /// `<kind>.<member>`.
    Member(String),
}

impl std::fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProtocolError::TooLong => write!(f, "a line longer than {MAX_LINE} bytes"),
            ProtocolError::NotUtf8 => write!(f, "a line that is not UTF-8"),
            ProtocolError::Json(e) => write!(f, "not JSON: {e}"),
            ProtocolError::NoKind => write!(f, "no string \"t\" member"),
            ProtocolError::UnknownKind(kind) => write!(f, "unknown message kind {kind:?}"),
            ProtocolError::Member(which) => write!(f, "{which} is missing or of the wrong type"),
        }
    }
}

impl std::error::Error for ProtocolError {}

fn obj(members: Vec<(&str, Value)>) -> Value {
    Value::Obj(
        members
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect(),
    )
}

fn num(n: u64) -> Value {
    Value::Num(n.to_string())
}

fn optional(n: Option<u64>) -> Value {
    n.map_or(Value::Null, num)
}

/// The line for a value: its JSON and a line feed, if it fits.
fn line(value: &Value) -> Result<String, ProtocolError> {
    let mut text = json::write(value);
    text.push('\n');
    if text.len() > MAX_LINE {
        return Err(ProtocolError::TooLong);
    }
    Ok(text)
}

/// The members a decoder reads, with errors that name them.
struct Fields<'a> {
    kind: &'a str,
    value: &'a Value,
}

impl<'a> Fields<'a> {
    fn of(text: &'a str, parsed: &'a mut Option<Value>) -> Result<Fields<'a>, ProtocolError> {
        if text.len() + 1 > MAX_LINE {
            return Err(ProtocolError::TooLong);
        }
        let value =
            parsed.insert(json::parse(text).map_err(|e| ProtocolError::Json(e.to_string()))?);
        let kind = value
            .get("t")
            .and_then(Value::as_str)
            .ok_or(ProtocolError::NoKind)?;
        Ok(Fields { kind, value })
    }

    fn missing(&self, member: &str) -> ProtocolError {
        ProtocolError::Member(format!("{}.{member}", self.kind))
    }

    fn text(&self, member: &str) -> Result<String, ProtocolError> {
        self.value
            .get(member)
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| self.missing(member))
    }

    fn number(&self, member: &str) -> Result<u64, ProtocolError> {
        self.value
            .get(member)
            .and_then(Value::as_num)
            .and_then(|n| n.parse().ok())
            .ok_or_else(|| self.missing(member))
    }

    fn optional(&self, member: &str) -> Result<Option<u64>, ProtocolError> {
        match self.value.get(member) {
            Some(Value::Null) => Ok(None),
            _ => self.number(member).map(Some),
        }
    }

    fn boolean(&self, member: &str) -> Result<bool, ProtocolError> {
        self.value
            .get(member)
            .and_then(Value::as_bool)
            .ok_or_else(|| self.missing(member))
    }

    fn object(&self, member: &str) -> Result<Value, ProtocolError> {
        match self.value.get(member) {
            Some(v @ Value::Obj(_)) => Ok(v.clone()),
            _ => Err(self.missing(member)),
        }
    }
}

impl FromSupervisor {
    /// The message as a JSON value, members in the documented order.
    pub fn to_value(&self) -> Value {
        match self {
            FromSupervisor::Hello {
                v,
                receiver,
                supervisor,
            } => obj(vec![
                ("t", Value::text("hello")),
                ("v", num(u64::from(*v))),
                ("receiver", num(*receiver as u64)),
                ("supervisor", Value::text(supervisor)),
            ]),
            FromSupervisor::Build(build) => obj(vec![
                ("t", Value::text("build")),
                ("present", Value::Bool(build.present)),
                (
                    "version",
                    Value::Str(build.version.chars().take(MAX_VERSION_CHARS).collect()),
                ),
                ("build_epoch", optional(build.build_epoch)),
                ("expires_epoch", optional(build.expires_epoch)),
            ]),
            FromSupervisor::Status(status) => obj(vec![
                ("t", Value::text("status")),
                ("state", Value::text(status.state.as_str())),
                ("target", Value::text(&status.target)),
                ("name", Value::text(&status.name)),
                ("detail", Value::text(&status.detail)),
                ("generation", num(status.generation)),
            ]),
            FromSupervisor::Event { generation, event } => obj(vec![
                ("t", Value::text("event")),
                ("generation", num(*generation)),
                ("event", event.clone()),
            ]),
        }
    }

    /// The message as one line, its line feed included. An `event` whose
    /// payload would pass [`MAX_LINE`] is [`ProtocolError::TooLong`]: the
    /// sender drops it and says so.
    pub fn encode(&self) -> Result<String, ProtocolError> {
        line(&self.to_value())
    }

    /// Read one line (without or with its line feed).
    pub fn decode(text: &str) -> Result<FromSupervisor, ProtocolError> {
        let text = text.strip_suffix('\n').unwrap_or(text);
        let mut parsed = None;
        let f = Fields::of(text, &mut parsed)?;
        Ok(match f.kind {
            "hello" => FromSupervisor::Hello {
                v: u32::try_from(f.number("v")?).map_err(|_| f.missing("v"))?,
                receiver: usize::try_from(f.number("receiver")?)
                    .map_err(|_| f.missing("receiver"))?,
                supervisor: f.text("supervisor")?,
            },
            "build" => FromSupervisor::Build(BuildReport {
                present: f.boolean("present")?,
                version: f.text("version")?,
                build_epoch: f.optional("build_epoch")?,
                expires_epoch: f.optional("expires_epoch")?,
            }),
            "status" => FromSupervisor::Status(StatusReport {
                state: State::from_wire(&f.text("state")?).ok_or_else(|| f.missing("state"))?,
                target: f.text("target")?,
                name: f.text("name")?,
                detail: f.text("detail")?,
                generation: f.number("generation")?,
            }),
            "event" => FromSupervisor::Event {
                generation: f.number("generation")?,
                event: f.object("event")?,
            },
            other => return Err(ProtocolError::UnknownKind(other.to_string())),
        })
    }
}

impl ToSupervisor {
    /// The message as a JSON value, members in the documented order.
    pub fn to_value(&self) -> Value {
        match self {
            ToSupervisor::Assign {
                generation,
                target,
                name,
            } => obj(vec![
                ("t", Value::text("assign")),
                ("generation", num(*generation)),
                ("target", Value::text(target)),
                ("name", Value::text(name)),
            ]),
            ToSupervisor::Release { generation } => obj(vec![
                ("t", Value::text("release")),
                ("generation", num(*generation)),
            ]),
            ToSupervisor::Command {
                generation,
                command,
            } => obj(vec![
                ("t", Value::text("command")),
                ("generation", num(*generation)),
                ("command", command.clone()),
            ]),
            ToSupervisor::Restart => obj(vec![("t", Value::text("restart"))]),
        }
    }

    /// The message as one line, its line feed included.
    pub fn encode(&self) -> Result<String, ProtocolError> {
        line(&self.to_value())
    }

    /// Read one line (without or with its line feed).
    pub fn decode(text: &str) -> Result<ToSupervisor, ProtocolError> {
        let text = text.strip_suffix('\n').unwrap_or(text);
        let mut parsed = None;
        let f = Fields::of(text, &mut parsed)?;
        Ok(match f.kind {
            "assign" => ToSupervisor::Assign {
                generation: f.number("generation")?,
                target: f.text("target")?,
                name: f.text("name")?,
            },
            "release" => ToSupervisor::Release {
                generation: f.number("generation")?,
            },
            "command" => ToSupervisor::Command {
                generation: f.number("generation")?,
                command: f.object("command")?,
            },
            "restart" => ToSupervisor::Restart,
            other => return Err(ProtocolError::UnknownKind(other.to_string())),
        })
    }
}

/// Splits a byte stream into lines under the [`MAX_LINE`] bound.
///
/// Feed it what a read returned, then take lines until `Ok(None)`. A line
/// that passes the bound, or is not UTF-8, is an error and the connection
/// should be dropped: there is no way to find the next message's start that
/// a peer could not abuse.
#[derive(Debug, Default)]
pub struct LineBuffer {
    buffer: Vec<u8>,
}

impl LineBuffer {
    /// An empty buffer.
    pub fn new() -> LineBuffer {
        LineBuffer::default()
    }

    /// Add bytes read from the socket.
    pub fn feed(&mut self, bytes: &[u8]) {
        self.buffer.extend_from_slice(bytes);
    }

    /// The next whole line, without its line feed (and without a carriage
    /// return before it). Empty lines are skipped.
    pub fn next_line(&mut self) -> Result<Option<String>, ProtocolError> {
        loop {
            let Some(end) = self.buffer.iter().position(|b| *b == b'\n') else {
                if self.buffer.len() >= MAX_LINE {
                    return Err(ProtocolError::TooLong);
                }
                return Ok(None);
            };
            if end + 1 > MAX_LINE {
                return Err(ProtocolError::TooLong);
            }
            let mut line: Vec<u8> = self.buffer.drain(..=end).collect();
            line.pop();
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            if line.is_empty() {
                continue;
            }
            return String::from_utf8(line)
                .map(Some)
                .map_err(|_| ProtocolError::NotUtf8);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn samples_from() -> Vec<FromSupervisor> {
        vec![
            FromSupervisor::Hello {
                v: VERSION,
                receiver: 3,
                supervisor: "0.1.0".into(),
            },
            FromSupervisor::Build(BuildReport {
                present: true,
                version: "Soloist 1.3.8.96, build 20260930, Linux/aarch64".into(),
                build_epoch: Some(1_790_726_400),
                expires_epoch: Some(1_798_502_400),
            }),
            FromSupervisor::Build(BuildReport {
                present: false,
                version: String::new(),
                build_epoch: None,
                expires_epoch: None,
            }),
            FromSupervisor::Status(StatusReport {
                state: State::Failed,
                target: "live:den+kitchen".into(),
                name: "Kitchen + Den \"quoted\" \u{e9}".into(),
                detail: "soloist exited 1; retry in 4 s".into(),
                generation: u64::MAX,
            }),
            FromSupervisor::Event {
                generation: 7,
                event: json::parse(r#"{"type":"volume_changed","volume":42}"#).unwrap(),
            },
        ]
    }

    fn samples_to() -> Vec<ToSupervisor> {
        vec![
            ToSupervisor::Assign {
                generation: 1,
                target: "room:kitchen".into(),
                name: "Kitchen".into(),
            },
            ToSupervisor::Release { generation: 2 },
            ToSupervisor::Command {
                generation: 3,
                command: crate::api::Command::Pause.to_value(),
            },
            ToSupervisor::Restart,
        ]
    }

    #[test]
    fn every_message_round_trips_as_one_line() {
        for message in samples_from() {
            let line = message.encode().unwrap();
            assert!(
                line.ends_with('\n') && line.matches('\n').count() == 1,
                "{line}"
            );
            assert_eq!(FromSupervisor::decode(&line), Ok(message.clone()));
            assert_eq!(FromSupervisor::decode(line.trim_end()), Ok(message));
        }
        for message in samples_to() {
            let line = message.encode().unwrap();
            assert!(
                line.ends_with('\n') && line.matches('\n').count() == 1,
                "{line}"
            );
            assert_eq!(ToSupervisor::decode(&line), Ok(message));
        }
    }

    #[test]
    fn every_state_has_one_spelling() {
        for state in [
            State::Idle,
            State::Starting,
            State::Running,
            State::Expired,
            State::Failed,
            State::NoBinary,
        ] {
            assert_eq!(State::from_wire(state.as_str()), Some(state));
        }
        assert_eq!(State::from_wire("Running"), None);
    }

    #[test]
    fn what_decode_refuses() {
        use ProtocolError::*;
        assert!(matches!(ToSupervisor::decode("{"), Err(Json(_))));
        assert_eq!(ToSupervisor::decode("{}"), Err(NoKind));
        assert_eq!(ToSupervisor::decode("[1]"), Err(NoKind));
        assert_eq!(
            ToSupervisor::decode(r#"{"t":"reboot"}"#),
            Err(UnknownKind("reboot".into()))
        );
        // One side's kinds are unknown to the other.
        assert_eq!(
            ToSupervisor::decode(r#"{"t":"hello","v":1,"receiver":0,"supervisor":"x"}"#),
            Err(UnknownKind("hello".into()))
        );
        assert_eq!(
            FromSupervisor::decode(r#"{"t":"restart"}"#),
            Err(UnknownKind("restart".into()))
        );
        for (line, member) in [
            (
                r#"{"t":"assign","target":"room:a","name":"A"}"#,
                "assign.generation",
            ),
            (
                r#"{"t":"assign","generation":-1,"target":"room:a","name":"A"}"#,
                "assign.generation",
            ),
            (
                r#"{"t":"assign","generation":1.5,"target":"room:a","name":"A"}"#,
                "assign.generation",
            ),
            (
                r#"{"t":"assign","generation":1,"target":7,"name":"A"}"#,
                "assign.target",
            ),
            (
                r#"{"t":"assign","generation":1,"target":"room:a"}"#,
                "assign.name",
            ),
            (r#"{"t":"release"}"#, "release.generation"),
            (
                r#"{"t":"command","generation":1,"command":"pause"}"#,
                "command.command",
            ),
        ] {
            assert_eq!(
                ToSupervisor::decode(line),
                Err(Member(member.into())),
                "{line}"
            );
        }
        for (line, member) in [
            (r#"{"t":"hello","v":1,"receiver":0}"#, "hello.supervisor"),
            (
                r#"{"t":"hello","v":99999999999,"receiver":0,"supervisor":"x"}"#,
                "hello.v",
            ),
            (
                r#"{"t":"build","present":1,"version":"","build_epoch":null,"expires_epoch":null}"#,
                "build.present",
            ),
            (
                r#"{"t":"build","present":true,"version":"","build_epoch":"x","expires_epoch":null}"#,
                "build.build_epoch",
            ),
            (
                r#"{"t":"build","present":true,"version":"","build_epoch":null}"#,
                "build.expires_epoch",
            ),
            (
                r#"{"t":"status","state":"napping","target":"","name":"","detail":"","generation":0}"#,
                "status.state",
            ),
            (r#"{"t":"event","generation":0,"event":[]}"#, "event.event"),
        ] {
            assert_eq!(
                FromSupervisor::decode(line),
                Err(Member(member.into())),
                "{line}"
            );
        }
    }

    #[test]
    fn an_unknown_member_is_ignored() {
        assert_eq!(
            ToSupervisor::decode(r#"{"t":"release","generation":4,"why":"later"}"#),
            Ok(ToSupervisor::Release { generation: 4 })
        );
    }

    #[test]
    fn the_line_bound_holds_both_ways() {
        // The largest event that fits, and one byte more.
        let frame = |pad: usize| FromSupervisor::Event {
            generation: 1,
            event: obj(vec![("type", Value::Str("x".repeat(pad)))]),
        };
        let overhead = frame(0).encode().unwrap().len();
        let fits = frame(MAX_LINE - overhead).encode().unwrap();
        assert_eq!(fits.len(), MAX_LINE);
        assert!(FromSupervisor::decode(&fits).is_ok());
        assert_eq!(
            frame(MAX_LINE - overhead + 1).encode(),
            Err(ProtocolError::TooLong)
        );
        let long = format!(r#"{{"t":"restart","pad":"{}"}}"#, "x".repeat(MAX_LINE));
        assert_eq!(ToSupervisor::decode(&long), Err(ProtocolError::TooLong));
    }

    #[test]
    fn a_version_is_cut_to_its_bound() {
        let line = FromSupervisor::Build(BuildReport {
            present: true,
            version: "\u{e9}".repeat(500),
            build_epoch: None,
            expires_epoch: None,
        })
        .encode()
        .unwrap();
        let FromSupervisor::Build(build) = FromSupervisor::decode(&line).unwrap() else {
            panic!("not a build");
        };
        assert_eq!(build.version.chars().count(), MAX_VERSION_CHARS);
    }

    #[test]
    fn the_line_buffer_splits_at_any_boundary() {
        let lines: Vec<String> = samples_to().iter().map(|m| m.encode().unwrap()).collect();
        let stream: Vec<u8> = lines.concat().into_bytes();
        for chunk in [1usize, 2, 3, 7, 64, stream.len()] {
            let mut buffer = LineBuffer::new();
            let mut got = Vec::new();
            for piece in stream.chunks(chunk) {
                buffer.feed(piece);
                while let Some(line) = buffer.next_line().unwrap() {
                    got.push(ToSupervisor::decode(&line).unwrap());
                }
            }
            assert_eq!(got, samples_to(), "chunks of {chunk}");
        }
    }

    #[test]
    fn the_line_buffer_skips_blank_lines_and_carriage_returns() {
        let mut buffer = LineBuffer::new();
        buffer.feed(b"\n\r\n{\"t\":\"restart\"}\r\n\n");
        assert_eq!(
            buffer.next_line(),
            Ok(Some("{\"t\":\"restart\"}".to_string()))
        );
        assert_eq!(buffer.next_line(), Ok(None));
    }

    #[test]
    fn the_line_buffer_refuses_an_endless_line_and_bad_utf8() {
        let mut buffer = LineBuffer::new();
        buffer.feed(&vec![b'x'; MAX_LINE - 1]);
        assert_eq!(buffer.next_line(), Ok(None));
        buffer.feed(b"x");
        assert_eq!(buffer.next_line(), Err(ProtocolError::TooLong));
        let mut buffer = LineBuffer::new();
        buffer.feed(&vec![b'x'; MAX_LINE]);
        buffer.feed(b"\n");
        assert_eq!(buffer.next_line(), Err(ProtocolError::TooLong));
        let mut buffer = LineBuffer::new();
        buffer.feed(&vec![b'x'; MAX_LINE - 1]);
        buffer.feed(b"\n");
        assert!(matches!(buffer.next_line(), Ok(Some(line)) if line.len() == MAX_LINE - 1));
        let mut buffer = LineBuffer::new();
        buffer.feed(&[0xFF, b'\n']);
        assert_eq!(buffer.next_line(), Err(ProtocolError::NotUtf8));
    }
}
