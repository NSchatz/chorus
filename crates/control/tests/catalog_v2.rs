//! Control catalog version 2: its committed vectors, both directions, and the
//! room model's rules, each one a named test.
//!
//! Goal 11's line A: "Rooms, bonded sets, saved and live groups,
//! take-the-room and group volume pass control-catalog fixture tests, and a
//! Wi-Fi endpoint is refused from a bonded set". Line B's server half: volume
//! limits and quiet hours hold on every volume path the room model has.
//!
//! The vectors are DISCOVERED, as `catalog_vectors.rs` discovers v1's: every
//! `.fields` under `fixtures/control/v2/` runs, and a v2 message type with no
//! vector turns this red. The v1 vectors are `catalog_vectors.rs`'s and are
//! untouched: a v2 build writes every v1 command at v1, byte for byte.

mod vectors;

use std::collections::BTreeSet;

use chorus_control::catalog::{
    decode_command, decode_message, is_server_id, Command, ControllerEvent, Refusal, ServerInfo,
    VoiceRun, VoiceWake, Volume, WakeWord,
};
use chorus_control::firmware::{Image, Report, SpeakerFirmware};
use chorus_control::json;
use chorus_control::rooms::{
    Alarm, Autoplay, BondMember, CivilTime, ClockTime, Days, InputId, InputKind, InputLabel,
    InputRole, Link, NowPlaying, Origin, PlayState, PlaybackAction, QuietWindow, Role,
    SoloistBuild, SoloistReceiver, SoloistState, Source, StoredKind, StoredSource,
};
use chorus_control::sound::{EqFilter, FixedPoint, Polarity};
use chorus_control::speakers::KeyChange;
use chorus_control::theater::TvUpmix;
use chorus_control::transport::{Transport, ZoneTransports};
use chorus_control::zones::{GroupKind, Zone, Zones};
use chorus_protocol::v2::{self as wire, ControllerCommand};
use chorus_server::controller::{translate, ControllerAction};

use vectors::{read_fields_in, read_json_in, v2_dir, vector_names_in, Fields};

/// Every message type catalog version 2 adds or changes, plus the ones the
/// server sends. Listed, not derived, so a new type has to be added here.
const EVERY_V2_MESSAGE_TYPE: &[&str] = &[
    "attach",
    "join",
    "bond",
    "unbond",
    "group_save",
    "group_delete",
    "take",
    "group_volume",
    "group_volume_step",
    "volume_step",
    "limit",
    "quiet_hours",
    "quiet_hours_enabled",
    "voice_enabled",
    "voice_wake_words",
    "voice_start",
    "voice_stop",
    "alarm_set",
    "alarm_delete",
    "alarm_stop",
    "sleep",
    "autoplay",
    "sound",
    "bass_management",
    "room_eq",
    "av_trim",
    "speaker_name",
    "speaker_room",
    "speaker_forget",
    "firmware_install",
    "firmware_cancel",
    "firmware_rescan",
    "source_store",
    "source_forget",
    "input_label",
    "soloist_restart",
    "playback",
    "announce",
    "server",
    "controller_event",
    "voice_wake",
    "voice_run",
    "state",
    "error",
    "refused",
];

fn v(t: u32) -> Volume {
    Volume::from_thousandths(i64::from(t)).unwrap()
}

fn list(text: &str) -> Vec<String> {
    text.split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
}

// --- the vectors ----------------------------------------------------------

#[test]
fn every_v2_message_type_has_a_committed_vector() {
    let mut covered = BTreeSet::new();
    for name in vector_names_in(&v2_dir()) {
        covered.insert(read_fields_in(&v2_dir(), &name).get("message_type"));
    }
    for t in EVERY_V2_MESSAGE_TYPE {
        assert!(
            covered.contains(*t),
            "catalog v2 has a '{}' message and fixtures/control/v2/ has no vector for it; \
             covered: {:?}",
            t,
            covered
        );
    }
    for t in &covered {
        assert!(
            EVERY_V2_MESSAGE_TYPE.contains(&t.as_str()),
            "fixtures/control/v2/ carries a vector for '{}', which this catalog does not have",
            t
        );
    }
}

#[test]
fn encoding_every_v2_vector_produces_its_committed_bytes() {
    let mut ran = 0;
    for name in vector_names_in(&v2_dir()) {
        let fields = read_fields_in(&v2_dir(), &name);
        assert_eq!(
            encode_from(&fields),
            read_json_in(&v2_dir(), &name),
            "fixtures/control/v2/{}.json is the contract and {}.fields did not produce it",
            name,
            name
        );
        ran += 1;
    }
    assert!(ran >= 40, "only {} v2 vectors ran", ran);
}

#[test]
fn decoding_every_v2_command_vector_recovers_its_committed_fields() {
    let mut ran = 0;
    for name in vector_names_in(&v2_dir()) {
        let fields = read_fields_in(&v2_dir(), &name);
        if matches!(
            fields.get("message_type").as_str(),
            "state"
                | "error"
                | "refused"
                | "server"
                | "controller_event"
                | "voice_wake"
                | "voice_run"
        ) {
            continue;
        }
        let bytes = read_json_in(&v2_dir(), &name);
        let (version, command) = decode_message(&bytes)
            .unwrap_or_else(|e| panic!("v2/{}.json does not decode: {}", name, e));
        assert_eq!(command, command_from(&fields), "v2/{}", name);
        assert_eq!(version, 2, "v2/{} is written at catalog version 2", name);
        // And the bytes are the encoder's own: decode then encode is identity.
        assert_eq!(command.encode(), bytes, "v2/{}", name);
        ran += 1;
    }
    assert!(ran >= 18, "only {} v2 command vectors ran", ran);
}

#[test]
fn every_v2_vector_is_json_and_ends_with_one_newline() {
    for name in vector_names_in(&v2_dir()) {
        json::parse(&read_json_in(&v2_dir(), &name))
            .unwrap_or_else(|e| panic!("v2/{}.json is not JSON: {}", name, e));
    }
}

#[test]
fn a_v2_build_refuses_an_unknown_version_naming_both_of_its_versions() {
    let refusal = decode_command(r#"{"v":9,"t":"hello"}"#).unwrap_err();
    assert!(refusal.ends_the_session());
    assert_eq!(
        refusal.encode(),
        read_json_in(&v2_dir(), "refused-unknown-version")
    );
    assert!(refusal.encode().contains(r#""implemented":[1,2]"#));
}

/// The server configured as a vector says: its zones, their transports.
fn server_from(fields: &Fields) -> Zones {
    let default_audio = if fields.has("default_audio") {
        fields.get("default_audio")
    } else {
        "127.0.0.1:4010".to_string()
    };
    let mut zones = Zones::new(&default_audio);
    for (key, value) in fields.pairs() {
        if let Some(group) = key.strip_prefix("group_audio.") {
            zones.set_group_audio(group, value);
        }
    }
    let mut declared = Vec::new();
    for id in list(&fields.get("zones")) {
        let key = format!("transport.{}", id);
        if fields.has(&key) {
            declared.push((id.clone(), Transport::parse(&fields.get(&key)).unwrap()));
        }
        zones.add(Zone::new(&id)).unwrap();
    }
    zones.set_transports(ZoneTransports::new(&declared));
    // (goal 18) The server's `--announce-origin` list:
    // `announce_origins = <origin>,<origin>`.
    if fields.has("announce_origins") {
        zones.set_announce_origins(
            list(&fields.get("announce_origins"))
                .iter()
                .map(|o| Origin::parse(o).unwrap())
                .collect(),
        );
    }
    // (voice, P8) The wake-word models the server runs:
    // `wake_words = <id>:<phrase>,<id>:<phrase>`.
    if fields.has("wake_words") {
        zones.set_wake_words(
            list(&fields.get("wake_words"))
                .iter()
                .map(|w| {
                    let (id, phrase) = w.split_once(':').unwrap();
                    WakeWord {
                        id: id.to_string(),
                        phrase: phrase.to_string(),
                    }
                })
                .collect(),
        );
    }
    // (ADR 0194) The built-in chimes the server can ring (`chimes =
    // <name>,<name>`), and whether a schedule runtime counts its sleep timers
    // down (`sleep_counted = 1`).
    if fields.has("chimes") {
        zones.set_chimes(list(&fields.get("chimes")));
    }
    if fields.has("sleep_counted") {
        zones.set_sleep_counted(true);
    }
    // (goal 14) What the session layer said before the commands: the ids it
    // adopted, each with its key's fingerprint (`adopted.N = <id> <key>`).
    let mut n = 0;
    while fields.has(&format!("adopted.{}", n)) {
        let line = fields.get(&format!("adopted.{}", n));
        let (id, key) = line.split_once(' ').unwrap();
        zones.speaker_adopted(id, key).unwrap();
        n += 1;
    }
    // (goal 14, firmware) The images the server staged (`image.N = <name> |
    // <version> | <board> | <size> | <sha256> | verified` or the refusal's
    // reason; `firmware_dir = 1` alone is a directory with nothing in it),
    // then the sessions up before the commands (`up.N = <id> | <software> |
    // <roles>`) and what each speaker reported (`report.N = <id> | <state> |
    // <reason> | <transfer> | <received> | <version> | <board> | <slot> |
    // <image_version> | <carried>`).
    let mut images = Vec::new();
    let mut n = 0;
    while fields.has(&format!("image.{}", n)) {
        let line = fields.get(&format!("image.{}", n));
        let p: Vec<&str> = line.split('|').map(str::trim).collect();
        images.push(Image {
            name: p[0].to_string(),
            version: p[1].to_string(),
            board: p[2].to_string(),
            size: p[3].parse().unwrap(),
            sha256: p[4].to_string(),
            refused: (p[5] != "verified").then(|| p[5].to_string()),
        });
        n += 1;
    }
    if n > 0 || fields.has("firmware_dir") {
        zones.set_firmware_images(Some(images));
    }
    let mut n = 0;
    while fields.has(&format!("up.{}", n)) {
        let line = fields.get(&format!("up.{}", n));
        let parts: Vec<&str> = line.split('|').map(str::trim).collect();
        zones.speaker_session_up(parts[0], parts[1], &list(parts[2]));
        n += 1;
    }
    let mut n = 0;
    while fields.has(&format!("report.{}", n)) {
        let line = fields.get(&format!("report.{}", n));
        let p: Vec<&str> = line.split('|').map(str::trim).collect();
        let report = Report {
            state: p[1].to_string(),
            reason: p[2].to_string(),
            transfer: p[3].parse().unwrap(),
            received: p[4].parse().unwrap(),
            version: p[5].to_string(),
            board: p[6].to_string(),
            slot: (p[7] != "none").then(|| p[7].parse().unwrap()),
            image_version: p[8].to_string(),
            carried: p[9] == "1",
        };
        zones.speaker_now(p[0], |now| {
            SpeakerFirmware::absorb(&mut now.firmware, &report)
        });
        n += 1;
    }
    let mut n = 0;
    while fields.has(&format!("command.{}", n)) {
        let text = fields.get(&format!("command.{}", n));
        let command = decode_command(&text).unwrap_or_else(|e| panic!("{}: {}", text, e));
        zones
            .apply(&command)
            .unwrap_or_else(|e| panic!("{}: {}", text, e));
        n += 1;
    }
    // And after them: the sessions that are up (`session.N = <id> | <software>
    // | <roles>`) and the key changes refused (`changed.N = <id> <pinned>
    // <offered>`).
    let mut n = 0;
    while fields.has(&format!("session.{}", n)) {
        let line = fields.get(&format!("session.{}", n));
        let parts: Vec<&str> = line.split('|').map(str::trim).collect();
        zones.speaker_session_up(parts[0], parts[1], &list(parts[2]));
        n += 1;
    }
    let mut n = 0;
    while fields.has(&format!("changed.{}", n)) {
        let line = fields.get(&format!("changed.{}", n));
        let parts: Vec<&str> = line.split(' ').collect();
        zones.speaker_key_changed(KeyChange {
            id: parts[0].to_string(),
            pinned: parts[1].to_string(),
            offered: parts[2].to_string(),
        });
        n += 1;
    }
    zones
}

/// (goal 17) What the receiver manager said: `soloist.N = <id> | <state> |
/// <target> | <name>`, `soloist_build = <version> | <days or empty>`,
/// `soloist_warning`, `soloist_exhausted = <target>,<target>`. A vector with
/// none of them is a server that runs no receiver.
fn soloist_from(fields: &Fields, zones: &mut Zones) {
    if !fields.has("soloist.0") {
        return;
    }
    let mut state = SoloistState::default();
    let mut n = 0;
    while fields.has(&format!("soloist.{}", n)) {
        let line = fields.get(&format!("soloist.{}", n));
        let p: Vec<&str> = line.split('|').map(str::trim).collect();
        state.receivers.push(SoloistReceiver {
            id: p[0].to_string(),
            state: p[1].to_string(),
            target: p[2].to_string(),
            name: p[3].to_string(),
        });
        n += 1;
    }
    if fields.has("soloist_build") {
        let line = fields.get("soloist_build");
        let (version, days) = line.split_once('|').unwrap();
        state.build = Some(SoloistBuild {
            version: version.trim().to_string(),
            expires_in_days: days.trim().parse().ok(),
        });
    }
    if fields.has("soloist_warning") {
        state.warning = Some(fields.get("soloist_warning"));
    }
    if fields.has("soloist_exhausted") {
        state.exhausted = list(&fields.get("soloist_exhausted"));
    }
    zones.set_soloist(Some(state));
}

fn encode_from(fields: &Fields) -> String {
    match fields.get("message_type").as_str() {
        "state" => {
            let mut zones = server_from(fields);
            if fields.has("time") {
                let time = fields.get("time");
                let (day, at) = time.split_once(' ').unwrap();
                let weekday = Days::from_names([day]).unwrap().mask().trailing_zeros() as u8;
                zones.set_civil_time(Some(CivilTime {
                    weekday,
                    time: ClockTime::parse(at).unwrap(),
                }));
            }
            if fields.has("offer_input") {
                zones.offer_input(InputId::parse(&fields.get("offer_input")).unwrap());
            }
            // (ADR 0194) The inputs the server offered with their kinds
            // (`input.N = <input> <kind>`), and what the runtime last counted
            // of each sleep timer (`sleep_remaining.N = <target> <seconds>`).
            let mut n = 0;
            while fields.has(&format!("input.{}", n)) {
                let line = fields.get(&format!("input.{}", n));
                let (input, kind) = line.split_once(' ').unwrap();
                let input = InputId::parse(input).unwrap();
                zones.set_input_kind(&input, InputKind::from_name(kind).unwrap());
                zones.offer_input(input);
                n += 1;
            }
            let mut n = 0;
            while fields.has(&format!("sleep_remaining.{}", n)) {
                let line = fields.get(&format!("sleep_remaining.{}", n));
                let (target, seconds) = line.split_once(' ').unwrap();
                zones.sleep_remaining(target, seconds.parse().unwrap());
                n += 1;
            }
            if fields.has("ringing") {
                zones
                    .set_alarm_ringing(&fields.get("ringing"), true)
                    .unwrap();
            }
            // (goal 16) What the runtime said each group's player source is
            // playing: `now_playing.N = <group> | <state> | <via> |
            // <duration_ms> | <title> | <artist> | <album> | <art_url>`, an
            // empty part where the fact is not known.
            let mut n = 0;
            while fields.has(&format!("now_playing.{}", n)) {
                let line = fields.get(&format!("now_playing.{}", n));
                let p: Vec<&str> = line.split('|').map(str::trim).collect();
                let text = |t: &str| (!t.is_empty()).then(|| t.to_string());
                zones
                    .set_now_playing(
                        p[0],
                        Some(NowPlaying {
                            title: text(p[4]),
                            artist: text(p[5]),
                            album: text(p[6]),
                            art_url: text(p[7]),
                            duration_ms: text(p[3]).map(|d| d.parse().unwrap()),
                            state: PlayState::parse(p[1]).unwrap(),
                            via: p[2].to_string(),
                        }),
                    )
                    .unwrap_or_else(|e| panic!("{}: {}", line, e));
                n += 1;
            }
            soloist_from(fields, &mut zones);
            zones.encode_state()
        }
        // (goal 18) What `GET /api/server` answers: `catalogs` and
        // `announce_origins` are comma-separated lists, the second possibly
        // empty.
        "server" => {
            let info = ServerInfo {
                id: fields.get("id"),
                software: fields.get("software"),
                catalogs: list(&fields.get("catalogs"))
                    .iter()
                    .map(|v| v.parse().unwrap())
                    .collect(),
                announce_origins: list(&fields.get("announce_origins"))
                    .iter()
                    .map(|o| Origin::parse(o).unwrap().literal())
                    .collect(),
            };
            assert!(is_server_id(&info.id), "{} is not a server id", info.id);
            info.encode()
        }
        // What `GET /api/controller-events` sends for one accepted button
        // press: the MQTT event's members behind the catalog's `v` and `t`.
        "controller_event" => ControllerEvent {
            endpoint: fields.get("endpoint"),
            zone: fields.get("zone"),
            command: fields.get("command"),
            value: fields.get("value").parse().unwrap(),
            target: fields.get("target"),
            outcome: fields.get("outcome"),
        }
        .encode(),
        // (voice, P8) What `GET /api/voice-events` sends when a room's
        // microphone heard a wake word, and what a `voice_start` is answered.
        "voice_wake" => VoiceWake {
            zone: fields.get("zone"),
            phrase: fields.get("phrase"),
        }
        .encode(),
        "voice_run" => VoiceRun {
            zone: fields.get("zone"),
            run: fields.get("run"),
            limit_ms: fields.get("limit_ms").parse().unwrap(),
        }
        .encode(),
        "error" | "refused" => {
            let mut zones = server_from(fields);
            soloist_from(fields, &mut zones);
            let refusal = match decode_message(&fields.get("input")) {
                Err(refusal) => refusal,
                Ok((version, command)) => {
                    let before = zones.encode_state();
                    let refusal = zones
                        .apply(&command)
                        .expect_err("this vector's input is refused")
                        .at(version);
                    assert_eq!(zones.encode_state(), before, "a refusal applies nothing");
                    refusal
                }
            };
            assert_eq!(refusal.field, fields.get("field"), "the field it names");
            assert_eq!(
                u8::from(refusal.ends_the_session()).to_string(),
                fields.get("ends_the_session")
            );
            refusal.encode()
        }
        _ => command_from(fields).encode(),
    }
}

fn command_from(fields: &Fields) -> Command {
    let get = |k: &str| fields.get(k);
    let flag = |k: &str| fields.get(k) == "1";
    let num = |k: &str| fields.get(k).parse::<i64>().unwrap();
    let vol = |k: &str| Volume::parse(&fields.get(k)).unwrap();
    match get("message_type").as_str() {
        "attach" => Command::Attach {
            zone: get("zone"),
            endpoint: get("endpoint"),
            link: Some(Link::parse(&get("link")).unwrap()),
        },
        "join" => Command::Join {
            zone: get("zone"),
            target: get("target"),
        },
        "bond" => Command::Bond {
            zone: get("zone"),
            members: list(&get("members"))
                .iter()
                .map(|pair| {
                    let (endpoint, role) = pair.split_once(':').unwrap();
                    BondMember {
                        endpoint: endpoint.to_string(),
                        role: Role::parse(role).unwrap(),
                    }
                })
                .collect(),
        },
        "unbond" => Command::Unbond { zone: get("zone") },
        "group_save" => Command::GroupSave {
            group: get("group"),
            name: get("name"),
            zones: list(&get("zones")),
        },
        "group_delete" => Command::GroupDelete {
            group: get("group"),
        },
        "take" => Command::Take {
            target: get("target"),
            source: fields
                .has("source")
                .then(|| Source::parse(&get("source")).unwrap()),
        },
        "group_volume" => Command::GroupVolume {
            group: get("group"),
            volume: vol("volume"),
        },
        "group_volume_step" => Command::GroupVolumeStep {
            group: get("group"),
            step: num("step") as i32,
        },
        "volume_step" => Command::VolumeStep {
            zone: get("zone"),
            step: num("step") as i32,
        },
        "limit" => Command::Limit {
            zone: get("zone"),
            limit: vol("limit"),
        },
        "quiet_hours" => {
            let mut windows = Vec::new();
            let mut n = 0;
            while fields.has(&format!("window.{}.days", n)) {
                let w = |k: &str| fields.get(&format!("window.{}.{}", n, k));
                windows.push(QuietWindow {
                    days: Days::from_names(w("days").split(',')).unwrap(),
                    start: ClockTime::parse(&w("start")).unwrap(),
                    end: ClockTime::parse(&w("end")).unwrap(),
                    limit: Volume::parse(&w("limit")).unwrap(),
                });
                n += 1;
            }
            Command::QuietHours {
                zone: get("zone"),
                windows,
            }
        }
        "quiet_hours_enabled" => Command::QuietHoursEnabled {
            zone: get("zone"),
            enabled: get("enabled") == "1",
        },
        "voice_enabled" => Command::VoiceEnabled {
            zone: get("zone"),
            enabled: get("enabled") == "1",
        },
        "voice_wake_words" => Command::VoiceWakeWords {
            zone: get("zone"),
            wake_words: list(&get("wake_words")),
        },
        "voice_start" => Command::VoiceStart { zone: get("zone") },
        "voice_stop" => Command::VoiceStop { zone: get("zone") },
        "alarm_set" => Command::AlarmSet(Alarm {
            id: get("alarm"),
            target: get("target"),
            time: ClockTime::parse(&get("time")).unwrap(),
            days: Days::from_names(list(&get("days")).iter().map(|s| s.as_str())).unwrap(),
            source: Source::parse(&get("source")).unwrap(),
            volume: vol("volume"),
            ramp_s: num("ramp_s") as u32,
            duration_min: num("duration_min") as u32,
            enabled: flag("enabled"),
        }),
        "alarm_delete" => Command::AlarmDelete {
            alarm: get("alarm"),
        },
        "alarm_stop" => Command::AlarmStop {
            alarm: get("alarm"),
        },
        "sleep" => Command::Sleep {
            target: get("target"),
            minutes: num("minutes") as u32,
        },
        "autoplay" => Command::Autoplay(Autoplay {
            input: InputId::parse(&get("input")).unwrap(),
            target: get("target"),
            enabled: flag("enabled"),
            // Goal 13: absent is true, as the decoder reads it.
            stop_on_standby: !fields.has("stop_on_standby") || flag("stop_on_standby"),
            low_latency: !fields.has("low_latency") || flag("low_latency"),
        }),
        "sound" => {
            let tone = |k: &str| fields.has(k).then(|| num(k) as i8);
            let flag = |k: &str| fields.has(k).then(|| fields.get(k) == "1");
            Command::Sound {
                zone: get("zone"),
                bass: tone("bass"),
                treble: tone("treble"),
                loudness: flag("loudness"),
                night: flag("night"),
                speech: flag("speech"),
                tv_upmix: fields
                    .has("tv_upmix")
                    .then(|| TvUpmix::parse(&get("tv_upmix")).unwrap()),
            }
        }
        "av_trim" => Command::AvTrim {
            zone: get("zone"),
            av_trim_ms: num("av_trim_ms") as i16,
        },
        "speaker_name" => Command::SpeakerName {
            speaker: get("speaker"),
            name: get("name"),
        },
        "speaker_room" => Command::SpeakerRoom {
            speaker: get("speaker"),
            // No `room` line is the wire's `null`: no room.
            room: fields.has("room").then(|| get("room")),
        },
        "speaker_forget" => Command::SpeakerForget {
            speaker: get("speaker"),
        },
        "firmware_install" => Command::FirmwareInstall {
            // No `speaker` line is the wire's `"all": true`.
            speaker: fields.has("speaker").then(|| get("speaker")),
            image: get("image"),
            force: fields.has("force") && flag("force"),
        },
        "firmware_cancel" => Command::FirmwareCancel {
            speaker: get("speaker"),
        },
        "firmware_rescan" => Command::FirmwareRescan,
        "soloist_restart" => Command::SoloistRestart,
        "playback" => Command::Playback {
            target: get("target"),
            action: PlaybackAction::parse(&get("action")).unwrap(),
        },
        "source_store" => Command::SourceStore(StoredSource {
            id: get("id"),
            kind: StoredKind::parse(&get("kind")).unwrap(),
            value: get("value"),
            name: get("name"),
        }),
        "source_forget" => Command::SourceForget { id: get("id") },
        "announce" => Command::Announce {
            target: get("target"),
            url: get("url"),
            volume: fields.has("volume").then(|| vol("volume")),
        },
        "input_label" => Command::InputLabel(InputLabel {
            input: InputId::parse(&get("input")).unwrap(),
            name: get("name"),
            role: InputRole::parse(&get("role")).unwrap(),
        }),
        "bass_management" => Command::BassManagement {
            zone: get("zone"),
            crossover_hz: fields
                .has("crossover_hz")
                .then(|| num("crossover_hz") as u16),
            sub_level_cdb: fields
                .has("sub_level_db")
                .then(|| FixedPoint::parse(&get("sub_level_db"), 2).unwrap() as i16),
            sub_polarity: fields
                .has("sub_polarity")
                .then(|| Polarity::parse(&get("sub_polarity")).unwrap()),
        },
        "room_eq" => Command::RoomEq {
            zone: get("zone"),
            // `filters` is `freq_hz gain_db q` per filter, `;` between them,
            // and empty for the empty list.
            filters: fields.has("filters").then(|| {
                get("filters")
                    .split(';')
                    .map(str::trim)
                    .filter(|f| !f.is_empty())
                    .map(|f| EqFilter::from_persisted(f).unwrap())
                    .collect()
            }),
            enabled: fields.has("enabled").then(|| flag("enabled")),
        },
        other => panic!("{} is not a v2 command", other),
    }
}

// --- the room model -------------------------------------------------------

/// A house: four rooms, the bedroom declared wireless, the living room's
/// three endpoints wired and the bedroom's on a radio.
fn house() -> Zones {
    let mut zones = Zones::new("127.0.0.1:4010");
    for id in ["living", "kitchen", "study", "bedroom"] {
        zones.add(Zone::new(id)).unwrap();
    }
    zones.set_transports(ZoneTransports::new(&[(
        "bedroom".to_string(),
        Transport::Wireless,
    )]));
    for (zone, endpoint, link) in [
        ("living", "endpoint-a", "wired"),
        ("living", "endpoint-b", "wired"),
        ("living", "endpoint-c", "wired"),
        ("living", "endpoint-w", "wireless"),
        ("bedroom", "endpoint-d", "wired"),
        ("bedroom", "endpoint-e", "wired"),
    ] {
        apply(
            &mut zones,
            &format!(
                r#"{{"v":2,"t":"attach","zone":"{}","endpoint":"{}","link":"{}"}}"#,
                zone, endpoint, link
            ),
        );
    }
    zones
}

fn apply(zones: &mut Zones, text: &str) {
    let command = decode_command(text).unwrap_or_else(|e| panic!("{}: {}", text, e));
    zones
        .apply(&command)
        .unwrap_or_else(|e| panic!("{}: {}", text, e));
}

/// Offer `text` and require a refusal naming `field`, with the state
/// byte-identical afterwards.
fn refused(zones: &mut Zones, text: &str, field: &str) -> Refusal {
    let before = zones.encode_state();
    let refusal = match decode_command(text) {
        Err(r) => r,
        Ok(command) => zones
            .apply(&command)
            .expect_err(&format!("'{}' has to be refused", text)),
    };
    assert_eq!(refusal.field, field, "{}: {}", text, refusal);
    assert_eq!(zones.encode_state(), before, "'{}' moved the state", text);
    refusal
}

fn volume_of(zones: &Zones, zone: &str) -> String {
    zones.zone(zone).unwrap().volume.literal()
}

fn group_of(zones: &Zones, zone: &str) -> String {
    zones.zone(zone).unwrap().group.clone()
}

#[test]
fn a_wireless_endpoint_is_refused_from_a_bonded_set() {
    let mut zones = house();
    let refusal = refused(
        &mut zones,
        r#"{"v":2,"t":"bond","zone":"living","members":[{"endpoint":"endpoint-a","role":"FL"},{"endpoint":"endpoint-w","role":"FR"}]}"#,
        "members",
    );
    assert!(
        refusal.detail.contains("endpoint-w") && refusal.detail.contains("wireless"),
        "the refusal names the endpoint and its link: {}",
        refusal
    );
    assert!(zones.zone("living").unwrap().bond.is_empty());
}

#[test]
fn an_endpoint_whose_link_is_unknown_is_refused_from_a_bonded_set() {
    let mut zones = house();
    apply(
        &mut zones,
        r#"{"v":1,"t":"attach","zone":"living","endpoint":"endpoint-u"}"#,
    );
    let refusal = refused(
        &mut zones,
        r#"{"v":2,"t":"bond","zone":"living","members":[{"endpoint":"endpoint-a","role":"FL"},{"endpoint":"endpoint-u","role":"FR"}]}"#,
        "members",
    );
    assert!(refusal.detail.contains("endpoint-u") && refusal.detail.contains("unknown"));
}

#[test]
fn a_room_declared_wireless_cannot_hold_a_bond_even_with_wired_endpoints() {
    let mut zones = house();
    let refusal = refused(
        &mut zones,
        r#"{"v":2,"t":"bond","zone":"bedroom","members":[{"endpoint":"endpoint-d","role":"FL"},{"endpoint":"endpoint-e","role":"FR"}]}"#,
        "zone",
    );
    assert!(refusal.detail.contains("declared wireless"), "{}", refusal);
}

#[test]
fn a_bonded_endpoint_that_reports_a_radio_is_refused_and_the_set_stays_wired() {
    let mut zones = house();
    apply(
        &mut zones,
        r#"{"v":2,"t":"bond","zone":"living","members":[{"endpoint":"endpoint-a","role":"FL"},{"endpoint":"endpoint-b","role":"FR"}]}"#,
    );
    refused(
        &mut zones,
        r#"{"v":2,"t":"attach","zone":"living","endpoint":"endpoint-b","link":"wireless"}"#,
        "link",
    );
    assert_eq!(zones.link("endpoint-b"), Link::Wired);
    // Unbonded, the same report is accepted.
    apply(&mut zones, r#"{"v":2,"t":"unbond","zone":"living"}"#);
    apply(
        &mut zones,
        r#"{"v":2,"t":"attach","zone":"living","endpoint":"endpoint-b","link":"wireless"}"#,
    );
    assert_eq!(zones.link("endpoint-b"), Link::Wireless);
}

#[test]
fn a_stereo_pair_with_a_sub_is_a_bonded_set_and_unbond_dissolves_it() {
    let mut zones = house();
    apply(
        &mut zones,
        r#"{"v":2,"t":"bond","zone":"living","members":[{"endpoint":"endpoint-a","role":"FL"},{"endpoint":"endpoint-b","role":"FR"},{"endpoint":"endpoint-c","role":"LFE"}]}"#,
    );
    let bond = &zones.zone("living").unwrap().bond;
    assert_eq!(bond.len(), 3);
    assert_eq!(bond[2].role, Role::Lfe);
    assert!(zones
        .encode_state()
        .contains(r#""bond":[{"endpoint":"endpoint-a","role":"FL"}"#));
    apply(&mut zones, r#"{"v":2,"t":"unbond","zone":"living"}"#);
    assert!(zones.zone("living").unwrap().bond.is_empty());
    // A room with no bond: unbond is not an error.
    apply(&mut zones, r#"{"v":2,"t":"unbond","zone":"living"}"#);
}

#[test]
fn a_bond_that_is_not_a_layout_or_not_the_rooms_own_is_refused() {
    let mut zones = house();
    for text in [
        // a centre alone with a front left is not a layout
        r#"{"v":2,"t":"bond","zone":"living","members":[{"endpoint":"endpoint-a","role":"FL"},{"endpoint":"endpoint-b","role":"FC"}]}"#,
        // two endpoints, one role
        r#"{"v":2,"t":"bond","zone":"living","members":[{"endpoint":"endpoint-a","role":"FL"},{"endpoint":"endpoint-b","role":"FL"}]}"#,
        // one endpoint, two roles
        r#"{"v":2,"t":"bond","zone":"living","members":[{"endpoint":"endpoint-a","role":"FL"},{"endpoint":"endpoint-a","role":"FR"}]}"#,
        // a role the channel map has and no layout uses
        r#"{"v":2,"t":"bond","zone":"living","members":[{"endpoint":"endpoint-a","role":"FL"},{"endpoint":"endpoint-b","role":"TFL"}]}"#,
        // an endpoint of another room
        r#"{"v":2,"t":"bond","zone":"living","members":[{"endpoint":"endpoint-a","role":"FL"},{"endpoint":"endpoint-d","role":"FR"}]}"#,
        // a member carrying a field nobody declared
        r#"{"v":2,"t":"bond","zone":"living","members":[{"endpoint":"endpoint-a","role":"FL","gain":1},{"endpoint":"endpoint-b","role":"FR"}]}"#,
    ] {
        refused(&mut zones, text, "members");
    }
}

#[test]
fn rooms_carry_limit_effective_limit_and_transport_in_the_state() {
    let zones = house();
    let state = zones.encode_state();
    assert!(state.starts_with(r#"{"v":2,"t":"state""#), "{}", state);
    assert!(state.contains(r#""transport":"wireless","limit":1.000,"effective_limit":1.000"#));
    // And the v1 shape of the same state is v1's, field for field.
    let v1 = zones.encode_state_at(1);
    assert!(v1.starts_with(r#"{"v":1,"t":"state""#));
    assert!(!v1.contains("effective_limit") && !v1.contains("\"groups\""));
}

#[test]
fn a_v2_only_command_at_v1_is_not_a_command_of_that_version() {
    let mut zones = house();
    let refusal = refused(
        &mut zones,
        r#"{"v":1,"t":"limit","zone":"kitchen","limit":0.500}"#,
        "t",
    );
    assert!(refusal.detail.contains("catalog version 1"), "{}", refusal);
    assert_eq!(refusal.version, 1, "and the refusal is written at v1");
    // A v1 command at v2 is accepted.
    apply(
        &mut zones,
        r#"{"v":2,"t":"volume","zone":"kitchen","volume":0.500}"#,
    );
    assert_eq!(volume_of(&zones, "kitchen"), "0.500");
}

#[test]
fn a_saved_group_is_listed_inactive_until_taken_and_survives_its_rooms_leaving() {
    let mut zones = house();
    apply(
        &mut zones,
        r#"{"v":2,"t":"group_save","group":"downstairs","name":"Downstairs","zones":["living","kitchen"]}"#,
    );
    let saved = zones.saved_groups()[0].clone();
    assert!(!zones.is_active(&saved), "saved, not yet taken");
    assert!(zones
        .encode_state()
        .contains(r#""saved_groups":[{"id":"downstairs","name":"Downstairs","zones":["living","kitchen"],"active":false}]"#));
    apply(&mut zones, r#"{"v":2,"t":"take","target":"downstairs"}"#);
    assert!(zones.is_active(&saved));
    assert_eq!(group_of(&zones, "living"), "downstairs");
    assert_eq!(group_of(&zones, "kitchen"), "downstairs");
    let formed = zones.formed_groups();
    assert_eq!(formed[0].kind, GroupKind::Saved);
    // A room leaving a saved group does not dissolve the definition.
    apply(&mut zones, r#"{"v":2,"t":"take","target":"kitchen"}"#);
    assert_eq!(
        group_of(&zones, "living"),
        "downstairs",
        "saved: not dissolved"
    );
    assert!(!zones.is_active(&saved));
    assert_eq!(zones.saved_groups().len(), 1, "still listed (K59)");
    apply(
        &mut zones,
        r#"{"v":2,"t":"group_delete","group":"downstairs"}"#,
    );
    assert!(zones.saved_groups().is_empty());
    refused(
        &mut zones,
        r#"{"v":2,"t":"group_delete","group":"downstairs"}"#,
        "group",
    );
    refused(
        &mut zones,
        r#"{"v":2,"t":"group_save","group":"study","name":"S","zones":["study","kitchen"]}"#,
        "group",
    );
    refused(
        &mut zones,
        r#"{"v":2,"t":"group_save","group":"up","name":"Up","zones":["study","attic"]}"#,
        "zones",
    );
}

#[test]
fn joining_a_room_that_is_alone_forms_a_live_group_with_an_assigned_id() {
    let mut zones = house();
    apply(
        &mut zones,
        r#"{"v":2,"t":"join","zone":"kitchen","target":"study"}"#,
    );
    assert_eq!(group_of(&zones, "kitchen"), "live-1");
    assert_eq!(group_of(&zones, "study"), "live-1");
    let live = zones
        .formed_groups()
        .into_iter()
        .find(|g| g.id == "live-1")
        .unwrap();
    assert_eq!(live.kind, GroupKind::Live);
    assert_eq!(live.zones, vec!["kitchen".to_string(), "study".to_string()]);
    // A third room joins by naming the group, or a room in it.
    apply(
        &mut zones,
        r#"{"v":2,"t":"join","zone":"living","target":"kitchen"}"#,
    );
    assert_eq!(group_of(&zones, "living"), "live-1");
    // A second live group takes the next free id.
    apply(&mut zones, r#"{"v":2,"t":"ungroup","zone":"living"}"#);
    apply(
        &mut zones,
        r#"{"v":2,"t":"join","zone":"bedroom","target":"living"}"#,
    );
    assert_eq!(group_of(&zones, "bedroom"), "live-2");
    assert_eq!(group_of(&zones, "living"), "live-2");
    refused(
        &mut zones,
        r#"{"v":2,"t":"join","zone":"bedroom","target":"attic"}"#,
        "target",
    );
}

#[test]
fn a_live_group_left_with_one_room_dissolves_into_that_room_still_playing() {
    let mut zones = house();
    apply(
        &mut zones,
        r#"{"v":2,"t":"join","zone":"kitchen","target":"study"}"#,
    );
    zones
        .set_group_source("live-1", Source::parse("chime:bell").unwrap())
        .unwrap();
    apply(
        &mut zones,
        r#"{"v":2,"t":"join","zone":"kitchen","target":"living"}"#,
    );
    assert_eq!(group_of(&zones, "study"), "study", "dissolved into its own");
    assert_eq!(
        zones.source("study").literal(),
        "chime:bell",
        "and it keeps playing what the live group played"
    );
    assert!(zones.formed_groups().iter().all(|g| g.id != "live-1"));
}

#[test]
fn take_moves_every_target_room_and_leftovers_keep_playing() {
    let mut zones = house();
    // living, kitchen and study play a line-in together.
    apply(
        &mut zones,
        r#"{"v":2,"t":"join","zone":"kitchen","target":"study"}"#,
    );
    apply(
        &mut zones,
        r#"{"v":2,"t":"join","zone":"living","target":"study"}"#,
    );
    zones
        .set_group_source(
            "live-1",
            Source::parse("line-in:endpoint-c/line-1").unwrap(),
        )
        .unwrap();
    // Casting to the kitchen takes it out of the group.
    apply(
        &mut zones,
        r#"{"v":2,"t":"take","target":"kitchen","source":"stream"}"#,
    );
    assert_eq!(group_of(&zones, "kitchen"), "kitchen");
    assert_eq!(
        group_of(&zones, "living"),
        "live-1",
        "two left: still a group"
    );
    assert_eq!(group_of(&zones, "study"), "live-1");
    assert_eq!(
        zones.source("live-1").literal(),
        "line-in:endpoint-c/line-1"
    );
    // Taking the study leaves the living room alone: the live group dissolves
    // into it, and it is still playing the line-in.
    apply(&mut zones, r#"{"v":2,"t":"take","target":"study"}"#);
    assert_eq!(group_of(&zones, "living"), "living");
    assert_eq!(
        zones.source("living").literal(),
        "line-in:endpoint-c/line-1"
    );
    assert_eq!(group_of(&zones, "study"), "study");
}

#[test]
fn take_of_a_room_moves_out_rooms_that_had_joined_its_own_group() {
    let mut zones = house();
    // v1's group command can put another room in the kitchen's own group.
    apply(
        &mut zones,
        r#"{"v":1,"t":"group","zone":"study","group":"kitchen"}"#,
    );
    zones
        .set_group_source("kitchen", Source::parse("chime:bell").unwrap())
        .unwrap();
    apply(
        &mut zones,
        r#"{"v":2,"t":"take","target":"kitchen","source":"stream"}"#,
    );
    assert_eq!(group_of(&zones, "kitchen"), "kitchen");
    assert_eq!(group_of(&zones, "study"), "study", "left behind, alone");
    assert_eq!(
        zones.source("study").literal(),
        "chime:bell",
        "still playing"
    );
    assert_eq!(zones.source("kitchen").literal(), "stream");
}

#[test]
fn take_of_a_saved_group_gathers_its_rooms_from_wherever_they_were() {
    let mut zones = house();
    apply(
        &mut zones,
        r#"{"v":2,"t":"join","zone":"kitchen","target":"study"}"#,
    );
    apply(
        &mut zones,
        r#"{"v":2,"t":"group_save","group":"front","name":"Front","zones":["living","kitchen"]}"#,
    );
    apply(
        &mut zones,
        r#"{"v":2,"t":"take","target":"front","source":"line-in:endpoint-c/line-1"}"#,
    );
    assert_eq!(group_of(&zones, "living"), "front");
    assert_eq!(group_of(&zones, "kitchen"), "front");
    assert_eq!(
        group_of(&zones, "study"),
        "study",
        "the live group dissolved"
    );
    assert_eq!(zones.source("front").literal(), "line-in:endpoint-c/line-1");
    refused(
        &mut zones,
        r#"{"v":2,"t":"take","target":"attic"}"#,
        "target",
    );
}

/// Sonos's definition, docs.sonos.com/docs/volume (read 2026-10-01): group
/// volume is the average of the players' volumes, and setting it adjusts
/// every player proportionally so the average is the level asked for.
#[test]
fn group_volume_is_the_average_and_setting_it_scales_every_room() {
    let mut zones = house();
    apply(
        &mut zones,
        r#"{"v":2,"t":"join","zone":"kitchen","target":"study"}"#,
    );
    apply(
        &mut zones,
        r#"{"v":2,"t":"volume","zone":"kitchen","volume":0.200}"#,
    );
    apply(
        &mut zones,
        r#"{"v":2,"t":"volume","zone":"study","volume":0.600}"#,
    );
    assert_eq!(zones.group_volume("live-1"), Some(v(400)), "the average");
    apply(
        &mut zones,
        r#"{"v":2,"t":"group_volume","group":"live-1","volume":0.200}"#,
    );
    assert_eq!(volume_of(&zones, "kitchen"), "0.100", "halved");
    assert_eq!(
        volume_of(&zones, "study"),
        "0.300",
        "halved: the balance is kept"
    );
    apply(
        &mut zones,
        r#"{"v":2,"t":"group_volume_step","group":"live-1","step":100}"#,
    );
    assert_eq!(volume_of(&zones, "kitchen"), "0.150");
    assert_eq!(volume_of(&zones, "study"), "0.450");
    assert_eq!(zones.group_volume("live-1"), Some(v(300)));
    // Each room stays individually adjustable.
    apply(
        &mut zones,
        r#"{"v":2,"t":"volume_step","zone":"study","step":-50}"#,
    );
    assert_eq!(volume_of(&zones, "study"), "0.400");
    // From a group volume of 0 every room is set to the level asked for.
    apply(
        &mut zones,
        r#"{"v":2,"t":"group_volume","group":"live-1","volume":0.000}"#,
    );
    apply(
        &mut zones,
        r#"{"v":2,"t":"group_volume","group":"live-1","volume":0.250}"#,
    );
    assert_eq!(volume_of(&zones, "kitchen"), "0.250");
    assert_eq!(volume_of(&zones, "study"), "0.250");
    // A step past either end is the end.
    apply(
        &mut zones,
        r#"{"v":2,"t":"group_volume_step","group":"live-1","step":-1000}"#,
    );
    assert_eq!(zones.group_volume("live-1"), Some(Volume::SILENT));
    refused(
        &mut zones,
        r#"{"v":2,"t":"group_volume","group":"nowhere","volume":0.500}"#,
        "group",
    );
}

/// Every path a volume reaches a room by, against one limited room. The
/// clamp is never a refusal.
#[test]
fn every_volume_path_is_clamped_to_the_limit_and_never_refused() {
    let mut zones = house();
    apply(
        &mut zones,
        r#"{"v":2,"t":"limit","zone":"kitchen","limit":0.400}"#,
    );
    assert_eq!(
        volume_of(&zones, "kitchen"),
        "0.400",
        "lowering a limit pulls down"
    );

    apply(
        &mut zones,
        r#"{"v":1,"t":"volume","zone":"kitchen","volume":0.900}"#,
    );
    assert_eq!(volume_of(&zones, "kitchen"), "0.400", "volume");

    apply(
        &mut zones,
        r#"{"v":2,"t":"volume","zone":"kitchen","volume":0.100}"#,
    );
    apply(
        &mut zones,
        r#"{"v":2,"t":"volume_step","zone":"kitchen","step":900}"#,
    );
    assert_eq!(volume_of(&zones, "kitchen"), "0.400", "volume_step");

    apply(
        &mut zones,
        r#"{"v":2,"t":"join","zone":"study","target":"kitchen"}"#,
    );
    apply(
        &mut zones,
        r#"{"v":2,"t":"volume","zone":"study","volume":0.400}"#,
    );
    apply(
        &mut zones,
        r#"{"v":2,"t":"group_volume","group":"live-1","volume":1.000}"#,
    );
    assert_eq!(volume_of(&zones, "kitchen"), "0.400", "group_volume");
    assert_eq!(
        volume_of(&zones, "study"),
        "1.000",
        "and the other room is not held"
    );
    apply(
        &mut zones,
        r#"{"v":2,"t":"volume","zone":"study","volume":0.400}"#,
    );
    apply(
        &mut zones,
        r#"{"v":2,"t":"group_volume_step","group":"live-1","step":500}"#,
    );
    assert_eq!(volume_of(&zones, "kitchen"), "0.400", "group_volume_step");

    for command in [wire::Command::VolumeSet, wire::Command::VolumeStep] {
        let pressed = ControllerCommand {
            command,
            value: 100,
            target: String::new(),
        };
        match translate(&zones, "kitchen", &pressed).unwrap() {
            ControllerAction::Apply(change) => zones.apply(&change).unwrap(),
            other => panic!("{:?}", other),
        }
        assert_eq!(
            volume_of(&zones, "kitchen"),
            "0.400",
            "controller {:?}",
            command
        );
    }

    assert_eq!(zones.runtime_volume("kitchen", v(1000)).unwrap(), v(400));
    assert_eq!(
        zones.start_ramp("kitchen", v(800)).unwrap(),
        v(400),
        "a ramp target"
    );
    assert_eq!(zones.zone("kitchen").unwrap().ramp, Some(v(400)));
    // Lowering the limit mid-ramp pulls the target down with the volume.
    apply(
        &mut zones,
        r#"{"v":2,"t":"limit","zone":"kitchen","limit":0.300}"#,
    );
    assert_eq!(zones.zone("kitchen").unwrap().ramp, Some(v(300)));
    assert_eq!(volume_of(&zones, "kitchen"), "0.300");
    // Raising it raises nothing.
    apply(
        &mut zones,
        r#"{"v":2,"t":"limit","zone":"kitchen","limit":1.000}"#,
    );
    assert_eq!(volume_of(&zones, "kitchen"), "0.300");
    zones.stop_ramp("kitchen").unwrap();
    assert_eq!(zones.zone("kitchen").unwrap().ramp, None);
}

fn at(day: &str, time: &str) -> Option<CivilTime> {
    Some(CivilTime {
        weekday: Days::from_names([day]).unwrap().mask().trailing_zeros() as u8,
        time: ClockTime::parse(time).unwrap(),
    })
}

#[test]
fn a_quiet_window_starting_pulls_the_volume_down_and_ending_raises_nothing() {
    let mut zones = house();
    apply(
        &mut zones,
        r#"{"v":2,"t":"quiet_hours","zone":"bedroom","windows":[{"days":["fri"],"start":"22:00","end":"07:00","limit":0.200},{"days":["sat"],"start":"23:00","end":"23:30","limit":0.100}]}"#,
    );
    apply(
        &mut zones,
        r#"{"v":2,"t":"volume","zone":"bedroom","volume":0.700}"#,
    );
    assert!(
        !zones.set_civil_time(at("fri", "21:59")),
        "not yet: nothing moves"
    );
    assert_eq!(volume_of(&zones, "bedroom"), "0.700");
    let serial = zones.serial();
    assert!(zones.set_civil_time(at("fri", "22:00")));
    assert!(zones.serial() > serial, "a window starting is a change");
    assert_eq!(volume_of(&zones, "bedroom"), "0.200");
    assert_eq!(zones.effective_limit("bedroom"), Some(v(200)));
    assert!(zones
        .encode_state()
        .contains(r#""limit":0.200,"active":true}"#));
    // Inside the window every path is held to its cap.
    apply(
        &mut zones,
        r#"{"v":2,"t":"volume","zone":"bedroom","volume":0.900}"#,
    );
    assert_eq!(volume_of(&zones, "bedroom"), "0.200");
    assert_eq!(zones.runtime_volume("bedroom", v(500)).unwrap(), v(200));
    // Past midnight it is still Friday's window.
    zones.set_civil_time(at("sat", "06:59"));
    assert_eq!(zones.effective_limit("bedroom"), Some(v(200)));
    // Ending raises nothing; the limit is the room's own again.
    zones.set_civil_time(at("sat", "07:00"));
    assert_eq!(zones.effective_limit("bedroom"), Some(Volume::FULL));
    assert_eq!(volume_of(&zones, "bedroom"), "0.200");
    // The lower of two caps wins while both apply.
    apply(
        &mut zones,
        r#"{"v":2,"t":"limit","zone":"bedroom","limit":0.150}"#,
    );
    zones.set_civil_time(at("sat", "23:10"));
    assert_eq!(zones.effective_limit("bedroom"), Some(v(100)));
    // The runtime may say which windows are active directly instead.
    zones.set_civil_time(None);
    assert_eq!(zones.effective_limit("bedroom"), Some(v(150)));
    assert!(zones.set_active_quiet("bedroom", &[true, false]).is_ok());
    assert_eq!(zones.effective_limit("bedroom"), Some(v(150)));
    assert!(zones.set_active_quiet("bedroom", &[true]).is_err());
}

#[test]
fn setting_quiet_hours_inside_a_window_applies_it_at_once() {
    let mut zones = house();
    zones.set_civil_time(at("mon", "23:00"));
    apply(
        &mut zones,
        r#"{"v":2,"t":"quiet_hours","zone":"kitchen","windows":[{"days":["mon"],"start":"22:00","end":"23:30","limit":0.300}]}"#,
    );
    assert_eq!(volume_of(&zones, "kitchen"), "0.300");
    apply(
        &mut zones,
        r#"{"v":2,"t":"quiet_hours","zone":"kitchen","windows":[]}"#,
    );
    assert_eq!(zones.effective_limit("kitchen"), Some(Volume::FULL));
}

#[test]
fn quiet_hours_switched_off_keep_their_windows_and_cap_nothing() {
    let mut zones = house();
    apply(
        &mut zones,
        r#"{"v":2,"t":"quiet_hours","zone":"kitchen","windows":[{"days":["mon"],"start":"22:00","end":"23:30","limit":0.300}]}"#,
    );
    let quiet_of = |zones: &Zones| {
        let state = zones.encode_state();
        let from = state.find(r#""id":"kitchen""#).unwrap();
        let start = from + state[from..].find(r#""quiet":"#).unwrap();
        let end = start + state[start..].find(r#","quiet_enabled""#).unwrap();
        state[start..end].to_string()
    };
    apply(
        &mut zones,
        r#"{"v":2,"t":"quiet_hours_enabled","zone":"kitchen","enabled":false}"#,
    );
    // The window starts while they are off: flagged, and not a cap.
    zones.set_civil_time(at("mon", "23:00"));
    assert_eq!(zones.effective_limit("kitchen"), Some(Volume::FULL));
    assert_eq!(volume_of(&zones, "kitchen"), "1.000");
    let windows = quiet_of(&zones);
    assert_eq!(
        windows,
        r#""quiet":[{"days":["mon"],"start":"22:00","end":"23:30","limit":0.300,"active":true}]"#
    );
    assert!(zones.encode_state().contains(r#""quiet_enabled":false"#));
    // No volume path is capped by it either.
    apply(
        &mut zones,
        r#"{"v":1,"t":"volume","zone":"kitchen","volume":0.900}"#,
    );
    assert_eq!(volume_of(&zones, "kitchen"), "0.900");
    // Switched back on inside the window: the cap applies at once.
    apply(
        &mut zones,
        r#"{"v":2,"t":"quiet_hours_enabled","zone":"kitchen","enabled":true}"#,
    );
    assert_eq!(zones.effective_limit("kitchen"), Some(v(300)));
    assert_eq!(volume_of(&zones, "kitchen"), "0.300");
    assert_eq!(quiet_of(&zones), windows, "the same windows, byte for byte");
    // Replacing the windows does not switch them back on or off.
    apply(
        &mut zones,
        r#"{"v":2,"t":"quiet_hours_enabled","zone":"kitchen","enabled":false}"#,
    );
    apply(
        &mut zones,
        r#"{"v":2,"t":"quiet_hours","zone":"kitchen","windows":[{"days":["mon"],"start":"22:00","end":"23:30","limit":0.100}]}"#,
    );
    assert_eq!(zones.effective_limit("kitchen"), Some(Volume::FULL));
    refused(
        &mut zones,
        r#"{"v":2,"t":"quiet_hours_enabled","zone":"kitchen"}"#,
        "enabled",
    );
    refused(
        &mut zones,
        r#"{"v":1,"t":"quiet_hours_enabled","zone":"kitchen","enabled":true}"#,
        "t",
    );
}

#[test]
fn alarms_sleep_and_autoplay_are_configured_and_validated_here() {
    let mut zones = house();
    apply(
        &mut zones,
        r#"{"v":2,"t":"alarm_set","alarm":"wake","target":"bedroom","time":"06:45","days":["mon","fri"],"source":"chime:bell","volume":0.300,"ramp_s":30,"duration_min":60,"enabled":true}"#,
    );
    assert_eq!(zones.alarms()[0].time.literal(), "06:45");
    zones.set_alarm_ringing("wake", true).unwrap();
    assert!(zones.is_ringing("wake"));
    apply(&mut zones, r#"{"v":2,"t":"alarm_stop","alarm":"wake"}"#);
    assert!(!zones.is_ringing("wake"));
    apply(&mut zones, r#"{"v":2,"t":"alarm_delete","alarm":"wake"}"#);
    assert!(zones.alarms().is_empty());
    refused(
        &mut zones,
        r#"{"v":2,"t":"alarm_delete","alarm":"wake"}"#,
        "alarm",
    );
    refused(
        &mut zones,
        r#"{"v":2,"t":"alarm_set","alarm":"wake","target":"bedroom","time":"06:45","days":[],"source":"chime:bell","volume":0.300,"ramp_s":601,"duration_min":60,"enabled":true}"#,
        "ramp_s",
    );

    apply(
        &mut zones,
        r#"{"v":2,"t":"sleep","target":"bedroom","minutes":30}"#,
    );
    assert_eq!(zones.sleep_timers()[0].minutes, 30);
    apply(
        &mut zones,
        r#"{"v":2,"t":"sleep","target":"bedroom","minutes":0}"#,
    );
    assert!(zones.sleep_timers().is_empty(), "0 cancels");
    apply(
        &mut zones,
        r#"{"v":2,"t":"join","zone":"kitchen","target":"study"}"#,
    );
    apply(
        &mut zones,
        r#"{"v":2,"t":"sleep","target":"live-1","minutes":15}"#,
    );
    assert!(zones.sleep_expired("live-1"));
    apply(
        &mut zones,
        r#"{"v":2,"t":"sleep","target":"live-1","minutes":15}"#,
    );
    apply(&mut zones, r#"{"v":2,"t":"ungroup","zone":"kitchen"}"#);
    apply(&mut zones, r#"{"v":2,"t":"ungroup","zone":"study"}"#);
    assert!(
        zones.sleep_timers().is_empty(),
        "a sleep timer on a group that is no longer formed goes with it"
    );

    apply(
        &mut zones,
        r#"{"v":2,"t":"autoplay","input":"endpoint-c/line-1","target":"living","enabled":true}"#,
    );
    assert_eq!(zones.autoplay_rules().len(), 1);
    assert!(zones.offer_input(InputId::parse("endpoint-c/line-1").unwrap()));
    assert!(!zones.offer_input(InputId::parse("endpoint-c/line-1").unwrap()));
    assert!(zones
        .encode_state()
        .contains(r#""inputs":["endpoint-c/line-1"]"#));
    assert!(zones.withdraw_input(&InputId::parse("endpoint-c/line-1").unwrap()));
}
