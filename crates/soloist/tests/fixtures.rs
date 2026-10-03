//! The Soloist vectors, `fixtures/soloist/` (Rust-only by declaration: the
//! supervisor and the server are the only speakers, there is no C
//! implementation; `tools/conventions/check-shared-fixtures.sh`,
//! `fixtures/README.md`).
//!
//! `command-*.json`, `event-*.json` and `entity.json` are the examples of
//! Soloist's WebSocket API reference page, typed from it verbatim (read
//! 2026-10-03), not produced by this crate: the builders must write those
//! values and the parser must read those fields. `composed-*.json` are the
//! two events the page gives only as a schema, composed from its own entity
//! and options examples. `version-*.txt` are the `--version` shapes users
//! transcribed in issue reports (LEADs). `protocol-*.line` pin the
//! supervisor protocol byte for byte.
//!
//! The one test below walks every kind and then fails if any file in the
//! directory was read by none of it, so a vector cannot sit there unused.

use std::collections::BTreeSet;
use std::path::PathBuf;

use chorus_control::json::{self, Value};
use chorus_soloist::api::{
    self, Action, Command, Cover, Entity, Event, Options, PlaybackState, Position, QueueEntry,
    Repeat, Status,
};
use chorus_soloist::build::{self, Expiry};
use chorus_soloist::protocol::{BuildReport, FromSupervisor, State, StatusReport, ToSupervisor};

struct Fixtures {
    dir: PathBuf,
    read: BTreeSet<String>,
}

impl Fixtures {
    fn new() -> Fixtures {
        Fixtures {
            dir: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/soloist"),
            read: BTreeSet::new(),
        }
    }

    /// The file exactly as committed.
    fn raw(&mut self, name: &str) -> String {
        self.read.insert(name.to_string());
        std::fs::read_to_string(self.dir.join(name)).unwrap_or_else(|e| panic!("{name}: {e}"))
    }

    fn json(&mut self, name: &str) -> Value {
        json::parse(&self.raw(name)).unwrap_or_else(|e| panic!("{name}: {e}"))
    }

    fn event(&mut self, name: &str) -> Event {
        api::parse_event(&self.raw(name)).unwrap_or_else(|e| panic!("{name}: {e}"))
    }

    /// Every file in the directory was read by some test above.
    fn all_read(&self) {
        let mut unread = Vec::new();
        for entry in std::fs::read_dir(&self.dir).expect("fixtures/soloist") {
            let name = entry.unwrap().file_name().into_string().unwrap();
            if !self.read.contains(&name) {
                unread.push(name);
            }
        }
        unread.sort();
        assert!(
            unread.is_empty(),
            "fixtures/soloist holds files no test reads: {unread:?}"
        );
    }
}

fn named(uri: &str, entity_type: &str, name: Option<&str>) -> Entity {
    Entity {
        uri: uri.to_string(),
        entity_type: entity_type.to_string(),
        name: name.map(str::to_string),
        ..Entity::default()
    }
}

fn commands(f: &mut Fixtures) {
    let cases = [
        ("command-get-state.json", Command::GetState),
        (
            "command-get-queue.json",
            Command::GetQueue { limit: Some(10) },
        ),
        ("command-play.json", Command::Play { uri: None }),
        (
            "command-play-uri.json",
            Command::Play {
                uri: Some("spotify:playlist:37i9dQZF1DXcBWIGoYBM5M".into()),
            },
        ),
        ("command-seek.json", Command::Seek { position_ms: 30000 }),
        ("command-set-volume.json", Command::SetVolume { volume: 50 }),
        (
            "command-set-shuffle.json",
            Command::SetShuffle { enabled: true },
        ),
        (
            "command-add-to-queue.json",
            Command::AddToQueue {
                uri: "spotify:track:6rqhFgbbKwnb9MLmUQDhG6".into(),
            },
        ),
    ];
    for (name, command) in cases {
        // Member for member, in the page's order: the value compares its
        // members as an ordered list.
        assert_eq!(command.to_value(), f.json(name), "{name}");
        assert_eq!(
            json::parse(&command.to_json()).unwrap(),
            f.json(name),
            "{name}"
        );
    }
}

fn position() -> Position {
    Position {
        position_ms: Some(45000),
        timestamp_ms: Some(1_747_654_321_000),
        speed: Some(1.0),
    }
}

fn playlist() -> Entity {
    named(
        "spotify:playlist:37i9dQZF1DXcBWIGoYBM5M",
        "playlist",
        Some("Today's Top Hits"),
    )
}

fn events(f: &mut Fixtures) {
    assert_eq!(
        f.event("event-command-result.json"),
        Event::CommandResult {
            command: "pause".into()
        }
    );
    assert_eq!(
        f.event("event-error.json"),
        Event::Error {
            message: "command requires authentication".into()
        }
    );
    assert_eq!(
        f.event("event-auth-state.json"),
        Event::AuthState {
            logged_in: true,
            is_active: Some(true),
            device_name: Some("Kitchen speaker".into()),
        }
    );
    let state = f.event("event-playback-state.json");
    assert_eq!(state.status(), Some(&Status::Playing));
    assert_eq!(
        state,
        Event::PlaybackState(Box::new(PlaybackState {
            status: Some(Status::Playing),
            item: Some(Entity {
                duration_ms: Some(210_000),
                ..named(
                    "spotify:track:2JRo0gjbX4GrCqBYdRohoo",
                    "track",
                    Some("My Song")
                )
            }),
            context: Some(playlist()),
            position: Some(position()),
            volume: Some(65),
            is_active: Some(true),
            options: Some(Options {
                shuffle: Some(false),
                repeat: Some(Repeat::Off),
                playback_speed: Some(1.0),
                modes: vec![],
            }),
            available_actions: vec![
                Action {
                    name: "pause".into(),
                    step_ms: None
                },
                Action {
                    name: "seek".into(),
                    step_ms: None
                },
                Action {
                    name: "seek_forward".into(),
                    step_ms: Some(15000)
                },
                Action {
                    name: "seek_backward".into(),
                    step_ms: Some(15000)
                },
            ],
        }))
    );

    // The entity envelope, carried by a track_changed (the page gives that
    // event as `{ "type": "track_changed", "item": Entity }`).
    let entity = f.raw("entity.json");
    let event = api::parse_event(&format!(
        r#"{{ "type": "track_changed", "item": {entity} }}"#
    ))
    .expect("track_changed");
    let Event::TrackChanged { item: Some(item) } = event else {
        panic!("track_changed without an item");
    };
    assert_eq!(
        item,
        Entity {
            uri: "spotify:track:2JRo0gjbX4GrCqBYdRohoo".into(),
            entity_type: "track".into(),
            name: Some("My Song".into()),
            covers: vec![Cover {
                url: "https://i.scdn.co/image/ab67616d00001e02...".into(),
                size: "large".into(),
            }],
            parent: Some(Box::new(named(
                "spotify:album:4aawyAB9vmqN3uQ7FjRGTy",
                "album",
                Some("Album Name")
            ))),
            creators: vec![named("spotify:artist:...", "artist", Some("Artist Name"))],
            duration_ms: Some(210_000),
            content_ratings: vec![],
        }
    );
    // What a now-playing record takes from it (the design's metadata rule).
    assert_eq!(item.name.as_deref(), Some("My Song"));
    assert_eq!(item.artist().as_deref(), Some("Artist Name"));
    assert_eq!(item.album(), Some("Album Name"));
    assert_eq!(
        item.cover_url(),
        Some("https://i.scdn.co/image/ab67616d00001e02...")
    );

    let paused = f.event("event-playback-changed.json");
    assert_eq!(paused.status(), Some(&Status::Paused));
    assert_eq!(
        paused,
        Event::PlaybackChanged {
            status: Some(Status::Paused)
        }
    );
    assert_eq!(
        f.event("event-volume-changed.json"),
        Event::VolumeChanged { volume: Some(42) }
    );
    assert_eq!(
        f.event("event-device-changed.json"),
        Event::DeviceChanged {
            is_active: Some(true),
            device_name: Some("Kitchen speaker".into()),
        }
    );
    assert_eq!(
        f.event("event-position-sync.json"),
        Event::PositionSync {
            position: Some(position())
        }
    );
    assert_eq!(
        f.event("event-queue-changed.json"),
        Event::QueueChanged {
            previous: vec![QueueEntry {
                uid: Some("spotify:track:previous".into()),
                source: Some("context".into()),
                item: Some(named("spotify:track:previous", "track", None)),
            }],
            upcoming: vec![QueueEntry {
                uid: Some("spotify:track:upcoming".into()),
                source: Some("queue".into()),
                item: Some(named("spotify:track:upcoming", "track", Some("Next Song"))),
            }],
        }
    );
    assert_eq!(
        f.event("composed-context-changed.json"),
        Event::ContextChanged {
            context: Some(playlist())
        }
    );
    assert_eq!(
        f.event("composed-options-changed.json"),
        Event::OptionsChanged {
            options: Some(Options {
                shuffle: Some(true),
                repeat: Some(Repeat::Context),
                playback_speed: Some(1.0),
                modes: vec![("context_enhancement".into(), "RECOMMENDATION".into())],
            })
        }
    );
}

fn versions(f: &mut Fixtures) {
    // 2026-09-30T00:00:00Z, the day issue #13's shape names.
    let sept30 = 1_790_726_400;
    for (name, epoch) in [
        ("version-issue-1.txt", Some(1_786_982_514)),
        ("version-issue-8.txt", Some(1_788_523_318)),
        ("version-issue-10.txt", Some(1_788_933_710)),
        ("version-issue-13.txt", Some(sept30)),
        ("version-garbage.txt", None),
    ] {
        let text = f.raw(name);
        let info = build::parse_version(&text);
        assert_eq!(info.build_epoch, epoch, "{name}");
        assert_eq!(info.version, text.lines().next().unwrap(), "{name}");
        assert_eq!(
            info.expires_epoch(),
            epoch.map(|e| e + 90 * 86_400),
            "{name}"
        );
    }
    let garbage = build::parse_version(&f.raw("version-garbage.txt"));
    assert_eq!(
        Expiry::at(garbage.expires_epoch(), u64::MAX),
        Expiry::Unknown
    );
}

fn protocol(f: &mut Fixtures) {
    let from = [
        (
            "protocol-hello.line",
            FromSupervisor::Hello {
                v: 1,
                receiver: 3,
                supervisor: "0.1.0".into(),
            },
        ),
        ("protocol-build.line", {
            let info = build::parse_version(&f.raw("version-issue-13.txt"));
            FromSupervisor::Build(BuildReport {
                present: true,
                expires_epoch: info.expires_epoch(),
                build_epoch: info.build_epoch,
                version: info.version,
            })
        }),
        ("protocol-build-unknown.line", {
            let info = build::parse_version(&f.raw("version-garbage.txt"));
            FromSupervisor::Build(BuildReport {
                present: true,
                expires_epoch: info.expires_epoch(),
                build_epoch: info.build_epoch,
                version: info.version,
            })
        }),
        (
            "protocol-build-absent.line",
            FromSupervisor::Build(BuildReport {
                present: false,
                version: String::new(),
                build_epoch: None,
                expires_epoch: None,
            }),
        ),
        (
            "protocol-status-idle.line",
            FromSupervisor::Status(StatusReport {
                state: State::Idle,
                target: String::new(),
                name: String::new(),
                detail: String::new(),
                generation: 0,
            }),
        ),
        (
            "protocol-status-running.line",
            FromSupervisor::Status(StatusReport {
                state: State::Running,
                target: "live:den+kitchen".into(),
                name: "Kitchen + Den".into(),
                detail: String::new(),
                generation: 12,
            }),
        ),
        (
            "protocol-status-failed.line",
            FromSupervisor::Status(StatusReport {
                state: State::Failed,
                target: "room:kitchen".into(),
                name: "Kitchen".into(),
                detail: "soloist exited 1; retry in 4 s".into(),
                generation: 7,
            }),
        ),
        (
            "protocol-event.line",
            FromSupervisor::Event {
                generation: 12,
                event: json::parse(r#"{"type":"playback_changed","status":"paused"}"#).unwrap(),
            },
        ),
    ];
    for (name, message) in from {
        let line = f.raw(name);
        assert_eq!(message.encode().as_deref(), Ok(line.as_str()), "{name}");
        assert_eq!(FromSupervisor::decode(&line), Ok(message), "{name}");
    }
    let to = [
        (
            "protocol-assign.line",
            ToSupervisor::Assign {
                generation: 12,
                target: "live:den+kitchen".into(),
                name: "Kitchen + Den".into(),
            },
        ),
        (
            "protocol-release.line",
            ToSupervisor::Release { generation: 13 },
        ),
        (
            "protocol-command.line",
            ToSupervisor::Command {
                generation: 12,
                command: Command::Play {
                    uri: Some("spotify:playlist:37i9dQZF1DXcBWIGoYBM5M".into()),
                }
                .to_value(),
            },
        ),
        ("protocol-restart.line", ToSupervisor::Restart),
    ];
    for (name, message) in to {
        let line = f.raw(name);
        assert_eq!(message.encode().as_deref(), Ok(line.as_str()), "{name}");
        assert_eq!(ToSupervisor::decode(&line), Ok(message), "{name}");
    }
    // The event a protocol line carries reads with the API model.
    let FromSupervisor::Event { event, .. } =
        FromSupervisor::decode(&f.raw("protocol-event.line")).unwrap()
    else {
        panic!("not an event");
    };
    assert_eq!(
        api::event_from_value(&event),
        Ok(Event::PlaybackChanged {
            status: Some(Status::Paused)
        })
    );
}

#[test]
fn every_soloist_vector_is_read_and_holds() {
    let mut f = Fixtures::new();
    commands(&mut f);
    events(&mut f);
    versions(&mut f);
    protocol(&mut f);
    f.all_read();
}
