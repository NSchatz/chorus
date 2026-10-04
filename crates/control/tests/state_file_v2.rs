//! The persisted state, format 2: everything catalog v2 configures comes back
//! after a restart, nothing that is a fact about now does, and a format 1
//! file (what every build before catalog v2 wrote) still loads unchanged.

use chorus_control::catalog::{decode_command, Volume};
use chorus_control::persist::{self, load, render, write_file, STATE_FORMAT};
use chorus_control::rooms::{CivilTime, ClockTime, InputId, Link};
use chorus_control::zones::{Zone, Zones};

fn apply(zones: &mut Zones, text: &str) {
    let command = decode_command(text).unwrap_or_else(|e| panic!("{}: {}", text, e));
    zones
        .apply(&command)
        .unwrap_or_else(|e| panic!("{}: {}", text, e));
}

/// A house with one of everything format 2 holds.
fn configured() -> Zones {
    let mut zones = Zones::new("127.0.0.1:4010");
    for id in ["living", "kitchen", "bedroom"] {
        zones.add(Zone::new(id)).unwrap();
    }
    for text in [
        r#"{"v":2,"t":"attach","zone":"living","endpoint":"endpoint-a","link":"wired"}"#,
        r#"{"v":2,"t":"attach","zone":"living","endpoint":"endpoint-b","link":"wired"}"#,
        r#"{"v":2,"t":"attach","zone":"bedroom","endpoint":"endpoint-c","link":"wireless"}"#,
        r#"{"v":2,"t":"bond","zone":"living","members":[{"endpoint":"endpoint-a","role":"FL"},{"endpoint":"endpoint-b","role":"FR"}]}"#,
        r#"{"v":1,"t":"name","zone":"living","name":"Living #1"}"#,
        r#"{"v":2,"t":"limit","zone":"kitchen","limit":0.450}"#,
        r#"{"v":2,"t":"quiet_hours","zone":"bedroom","windows":[{"days":["mon","fri"],"start":"22:00","end":"07:00","limit":0.200},{"days":["sun"],"start":"13:00","end":"15:00","limit":0.300}]}"#,
        r#"{"v":2,"t":"group_save","group":"downstairs","name":"Down #stairs","zones":["living","kitchen"]}"#,
        r#"{"v":2,"t":"take","target":"downstairs","source":"chime:bell"}"#,
        r#"{"v":2,"t":"alarm_set","alarm":"wake","target":"bedroom","time":"06:45","days":[],"source":"line-in:endpoint-c/line-1","volume":0.300,"ramp_s":30,"duration_min":0,"enabled":false}"#,
        r#"{"v":2,"t":"autoplay","input":"endpoint-c/line-1","target":"downstairs","enabled":true}"#,
        r#"{"v":2,"t":"sleep","target":"bedroom","minutes":20}"#,
    ] {
        apply(&mut zones, text);
    }
    zones
}

#[test]
fn everything_format_2_holds_comes_back_byte_for_byte() {
    let zones = configured();
    let text = render(&zones);
    assert!(text.contains(&format!("format = {}\n", STATE_FORMAT)));
    assert!(
        text.contains("bond = FL:endpoint-a,FR:endpoint-b\n"),
        "{}",
        text
    );
    assert!(text.contains("quiet = mon,fri 22:00-07:00 0.200; sun 13:00-15:00 0.300\n"));
    assert!(text.contains("[saved-group downstairs]\nname = Down \\#stairs\n"));
    assert!(text.contains("[autoplay endpoint-c/line-1]\n"));
    let back = load(&text, "127.0.0.1:4010").expect("it reads back");
    assert_eq!(render(&back), text, "and renders back to the same file");
    assert_eq!(back.zone("kitchen").unwrap().limit.literal(), "0.450");
    assert_eq!(back.zone("living").unwrap().bond.len(), 2);
    assert_eq!(back.zone("bedroom").unwrap().quiet.len(), 2);
    assert_eq!(back.link("endpoint-c"), Link::Wireless);
    assert_eq!(back.saved_groups()[0].name, "Down #stairs");
    assert_eq!(back.alarms()[0].days.names().len(), 0, "a one-shot alarm");
    assert!(!back.alarms()[0].enabled);
    assert_eq!(back.autoplay_rules()[0].target, "downstairs");
}

#[test]
fn what_is_a_fact_about_now_is_not_persisted() {
    let mut zones = configured();
    zones.set_civil_time(Some(CivilTime {
        weekday: 4,
        time: ClockTime::parse("23:00").unwrap(),
    }));
    zones.set_alarm_ringing("wake", true).unwrap();
    zones.offer_input(InputId::parse("endpoint-c/line-1").unwrap());
    zones.start_ramp("bedroom", Volume::FULL).unwrap();
    let back = load(&render(&zones), "x").unwrap();
    assert!(back.zones().iter().all(|z| z.present.is_empty()));
    assert!(back
        .zone("bedroom")
        .unwrap()
        .quiet_active
        .iter()
        .all(|a| !a));
    assert_eq!(back.zone("bedroom").unwrap().ramp, None);
    assert!(!back.is_ringing("wake"));
    assert!(back.inputs().is_empty());
    assert!(
        back.sleep_timers().is_empty(),
        "a countdown cannot be resumed"
    );
    assert_eq!(
        back.source("downstairs").literal(),
        "stream",
        "what a group plays is the runtime's to restore"
    );
    // But the volume a window pulled down IS the volume, and comes back.
    assert_eq!(back.zone("bedroom").unwrap().volume.literal(), "0.200");
}

/// A file as goal 10's build wrote it, verbatim.
const FORMAT_1: &str = "# chorus zone state, written by chorus-server.\n\
format = 1\n\
serial = 9\n\
\n\
[zone kitchen]\n\
name = Kitchen \\#1\n\
group = downstairs\n\
volume = 0.375\n\
muted = 0\n\
endpoints = endpoint-a,endpoint-b\n\
\n\
[zone study]\n\
name = Study\n\
group = downstairs\n\
volume = 1.000\n\
muted = 1\n\
endpoints =\n";

#[test]
fn a_format_1_file_loads_unchanged_and_is_written_back_as_the_current_format() {
    let zones = load(FORMAT_1, "127.0.0.1:4010").expect("format 1 still loads");
    assert_eq!(zones.serial(), 9);
    let kitchen = zones.zone("kitchen").unwrap();
    assert_eq!(kitchen.name, "Kitchen #1");
    assert_eq!(kitchen.group, "downstairs");
    assert_eq!(kitchen.volume.literal(), "0.375");
    assert_eq!(kitchen.endpoints, vec!["endpoint-a", "endpoint-b"]);
    assert_eq!(kitchen.limit, Volume::FULL, "the v2 defaults");
    assert!(kitchen.quiet.is_empty() && kitchen.bond.is_empty());
    assert!(zones.zone("study").unwrap().muted);
    // Its v1 state is what that build served.
    assert_eq!(
        zones.encode_state_at(1),
        r#"{"v":1,"t":"state","serial":9,"zones":[{"id":"kitchen","name":"Kitchen #1","group":"downstairs","volume":0.375,"muted":false,"endpoints":["endpoint-a","endpoint-b"],"present":[],"audio":"127.0.0.1:4010"},{"id":"study","name":"Study","group":"downstairs","volume":1.000,"muted":true,"endpoints":[],"present":[],"audio":"127.0.0.1:4010"}]}"#
    );
    let again = render(&zones);
    // Format 2 when catalog v2 wrote this test; format 3 since goal 12
    // (per-room sound), whose own test is tests/sound_v2.rs.
    assert!(again.contains(&format!("format = {}\n", STATE_FORMAT)));
    assert_eq!(
        render(&load(&again, "x").unwrap()),
        again,
        "and that one reads back"
    );
}

#[test]
fn a_format_1_file_cannot_carry_a_format_2_section_or_field_by_accident() {
    let text = format!("{}\n[alarm wake]\ntarget = kitchen\n", FORMAT_1);
    let err = load(&text, "x").unwrap_err();
    assert!(err.to_string().contains("added in format 2"), "{}", err);
}

#[test]
fn a_format_2_zone_missing_a_format_2_field_is_refused_not_defaulted() {
    let text = "format = 2\nserial = 1\n\n[zone kitchen]\nname = K\ngroup = kitchen\n\
                volume = 1.000\nmuted = 0\nendpoints =\nlimit = 1.000\nquiet =\n";
    let err = load(text, "x").unwrap_err();
    assert!(err.to_string().contains("has no 'bond'"), "{}", err);
}

#[test]
fn a_hand_edited_bond_holding_a_radio_is_refused_at_load() {
    let mut text = render(&configured());
    text = text.replace(
        "[endpoint endpoint-a]\nlink = wired",
        "[endpoint endpoint-a]\nlink = wireless",
    );
    let err = load(&text, "x").unwrap_err();
    assert!(
        err.to_string().contains("bonded set this build refuses")
            && err.to_string().contains("endpoint-a"),
        "{}",
        err
    );
}

#[test]
fn a_write_is_synced_renamed_and_leaves_no_temporary() {
    let directory = std::env::temp_dir().join(format!("chorus-state-v2-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("zones.state");
    let zones = configured();
    write_file(&path, &zones).expect("it writes");
    let back = persist::read_file(&path, "x")
        .unwrap()
        .expect("it is there");
    assert_eq!(render(&back), render(&zones));
    let leftovers: Vec<_> = std::fs::read_dir(&directory)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    assert_eq!(
        leftovers,
        vec!["zones.state".to_string()],
        "no temporary left"
    );
    let _ = std::fs::remove_dir_all(&directory);
}

// --- format 7: quiet hours switched off and on --------------------------------

const BEDROOM_WINDOWS: &str = "quiet = mon,fri 22:00-07:00 0.200; sun 13:00-15:00 0.300\n";

fn friday_night() -> Option<CivilTime> {
    Some(CivilTime {
        weekday: 4,
        time: ClockTime::parse("23:00").unwrap(),
    })
}

#[test]
fn quiet_hours_switched_off_and_their_windows_survive_a_restart() {
    let directory =
        std::env::temp_dir().join(format!("chorus-state-quiet-enabled-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("zones.state");
    let mut zones = configured();
    apply(
        &mut zones,
        r#"{"v":2,"t":"quiet_hours_enabled","zone":"bedroom","enabled":false}"#,
    );
    write_file(&path, &zones).expect("it writes");
    let saved = std::fs::read_to_string(&path).unwrap();
    assert_eq!(STATE_FORMAT, 8);
    assert!(saved.contains("format = 8\n"), "{}", saved);
    assert!(saved.contains(BEDROOM_WINDOWS), "{}", saved);
    assert!(saved.contains("quiet_enabled = 0\n"), "{}", saved);

    // The server that replaces it reads the file once, at start.
    let mut back = persist::read_file(&path, "127.0.0.1:4010")
        .unwrap()
        .expect("it is there");
    std::fs::remove_dir_all(&directory).unwrap();
    assert_eq!(render(&back), saved, "the same file, byte for byte");
    let bedroom = back.zone("bedroom").unwrap();
    assert!(!bedroom.quiet_enabled, "still switched off");
    assert_eq!(bedroom.quiet, zones.zone("bedroom").unwrap().quiet);
    assert!(back.zone("kitchen").unwrap().quiet_enabled, "the default");
    // Inside a window the restarted server still does not cap the room...
    back.set_civil_time(friday_night());
    assert_eq!(back.effective_limit("bedroom"), Some(Volume::FULL));
    // ...until they are switched back on, with the windows it had.
    apply(
        &mut back,
        r#"{"v":2,"t":"quiet_hours_enabled","zone":"bedroom","enabled":true}"#,
    );
    assert_eq!(back.effective_limit("bedroom").unwrap().literal(), "0.200");
    assert!(render(&back).contains(BEDROOM_WINDOWS));
    assert!(render(&back).contains("quiet_enabled = 1\n"));
}

#[test]
fn a_format_6_file_loads_with_quiet_hours_enabled_and_is_written_back_as_format_8() {
    // What the build before this one wrote: format 6, no `quiet_enabled`.
    let seven = render(&configured());
    let six = seven
        .replace("format = 8\n", "format = 6\n")
        .replace("quiet_enabled = 1\n", "");
    assert!(six.contains("format = 6\n") && !six.contains("quiet_enabled ="));
    let mut back = load(&six, "127.0.0.1:4010").expect("format 6 still loads");
    assert!(back.zones().iter().all(|z| z.quiet_enabled));
    assert_eq!(back.zone("bedroom").unwrap().quiet.len(), 2);
    back.set_civil_time(friday_night());
    assert_eq!(
        back.effective_limit("bedroom").unwrap().literal(),
        "0.200",
        "its windows cap the room as they did"
    );
    assert_eq!(
        render(&load(&six, "x").unwrap()),
        seven,
        "the next write is format 8"
    );
    // A format 7 file says it for every room: a missing field is refused,
    // not defaulted.
    let missing = seven.replacen("quiet_enabled = 1\n", "", 1);
    let error = load(&missing, "x").unwrap_err();
    assert!(error.to_string().contains("quiet_enabled"), "{}", error);
}
