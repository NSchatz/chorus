//! The schedule runtime against a modelled house, on modelled time.
//!
//! Every test here drives `chorus_server::schedule_runtime::Runtime` with a
//! monotonic instant and a civil one it is handed, never a clock, so each is
//! deterministic and runs in milliseconds whatever instant of whatever year it
//! models. The time zones are the committed TZif fixtures under
//! `fixtures/schedule/` (ADR 0072), read as bytes.
//!
//! Every call goes through [`House::call`], which holds the runtime to the
//! clamp on every report: a `room_volume`'s gain is never above the room's
//! effective limit (nor its own `limit` field), and no room's volume ever is.
//! That is the "every volume the runtime sets goes through the model's clamp"
//! property, asserted on every step of every test rather than in one.
//!
//! The test names are the evidence the goal asks for: alarms with chimes and
//! with a line-in, and sleep timers, work with volume ramps; line-in autoplay
//! works.

use chorus_control::rooms::{
    Alarm, Autoplay, ClockTime, Days, InputId, InputLabel, InputRole, QuietWindow, Source,
    StoredKind, StoredSource,
};
use chorus_control::zones::Zone as Room;
use chorus_control::{Command, Volume, Zones};
use chorus_schedule::civil::days_from_civil;
use chorus_schedule::Zone;
use chorus_server::schedule_runtime::{Effect, InputAction, Runtime, AUTOPLAY_HOLD_MS, STEP_MS};

const MS: u64 = 1_000_000;
const S: u64 = 1_000_000_000;

fn vol(t: u16) -> Volume {
    Volume::from_thousandths(i64::from(t)).unwrap()
}

fn utc(year: i64, month: u32, day: u32, hour: i64, minute: i64, second: i64) -> i64 {
    days_from_civil(year, month, day) * 86_400 + hour * 3600 + minute * 60 + second
}

fn fixture_zone(name: &str) -> Zone {
    let path = format!(
        "{}/../../fixtures/schedule/{}.slim.tzif",
        env!("CARGO_MANIFEST_DIR"),
        name
    );
    Zone::from_tzif(&std::fs::read(&path).unwrap()).unwrap()
}

fn input(text: &str) -> InputId {
    InputId::parse(text).unwrap()
}

/// An alarm at `time` on every day, ringing for `duration_min`.
fn alarm(id: &str, target: &str, time: &str, source: &str, volume: u16, ramp_s: u32) -> Alarm {
    Alarm {
        id: id.into(),
        target: target.into(),
        time: ClockTime::parse(time).unwrap(),
        days: Days::from_mask(0x7f).unwrap(),
        source: Source::parse(source).unwrap(),
        volume: vol(volume),
        ramp_s,
        duration_min: 1,
        enabled: true,
    }
}

/// A modelled house: the zones, the runtime, both clocks, and everything the
/// runtime has said.
struct House {
    zones: Zones,
    rt: Runtime,
    mono: u64,
    /// The civil instant at mono 0; a clock step moves it.
    utc_base: i64,
    said: Vec<Effect>,
}

impl House {
    fn new(tz: Zone, start_utc: i64, rooms: &[(&str, u16)]) -> House {
        let mut zones = Zones::new("127.0.0.1:4010");
        for (id, v) in rooms {
            let mut room = Room::new(id);
            room.volume = vol(*v);
            zones.add(room).unwrap();
        }
        let mut house = House {
            zones,
            rt: Runtime::new(tz),
            mono: 1_000 * S,
            utc_base: start_utc - 1_000,
            said: Vec::new(),
        };
        house.tick();
        house
    }

    fn utc(&self) -> i64 {
        self.utc_base + (self.mono / S) as i64
    }

    /// Hold one batch of effects to the clamp, and keep it.
    fn check(&mut self, out: Vec<Effect>) -> Vec<Effect> {
        for e in &out {
            if let Effect::RoomVolume {
                zone,
                gain,
                limit,
                ramp_ms,
            } = e
            {
                let cap = self.zones.effective_limit(zone).unwrap().thousandths() as u16;
                assert!(gain <= limit, "{e:?}: gain above its own limit");
                assert!(*gain <= cap, "{e:?}: gain above the effective limit {cap}");
                assert!(*ramp_ms <= 60_000, "{e:?}");
            }
        }
        for z in self.zones.zones() {
            assert!(z.volume <= z.effective_limit(), "{} above its limit", z.id);
        }
        self.said.extend(out.iter().cloned());
        out
    }

    fn tick(&mut self) -> Vec<Effect> {
        let (mono, utc) = (self.mono, self.utc());
        let out = self.rt.tick(mono, utc, &mut self.zones);
        self.check(out)
    }

    /// Advance both clocks by `total_ms`, ticking every `every_ms`.
    fn run(&mut self, total_ms: u64, every_ms: u64) -> Vec<Effect> {
        let mut out = Vec::new();
        let mut done = 0;
        while done < total_ms {
            let step = every_ms.min(total_ms - done);
            self.mono += step * MS;
            done += step;
            out.extend(self.tick());
        }
        out
    }

    /// A person's command, applied and then reported to the runtime.
    fn command(&mut self, command: Command) -> Vec<Effect> {
        self.zones.apply(&command).unwrap();
        let out = self
            .rt
            .on_command_applied(&command, self.mono, &mut self.zones);
        self.check(out)
    }

    fn signal(&mut self, id: &str, on: bool) -> Vec<Effect> {
        let out = self
            .rt
            .on_input_signal(&input(id), on, self.mono, &mut self.zones);
        self.check(out)
    }

    fn gone(&mut self, id: &str) -> Vec<Effect> {
        let out = self
            .rt
            .on_input_gone(&input(id), self.mono, &mut self.zones);
        self.check(out)
    }

    fn volume(&self, zone: &str) -> u16 {
        self.zones.zone(zone).unwrap().volume.thousandths() as u16
    }

    fn group(&self, zone: &str) -> String {
        self.zones.zone(zone).unwrap().group.clone()
    }

    fn source_of(&self, zone: &str) -> Source {
        self.zones.source(&self.group(zone))
    }

    fn logged(&self, needle: &str) -> usize {
        self.said
            .iter()
            .filter(|e| matches!(e, Effect::Log(l) if l.contains(needle)))
            .count()
    }
}

/// The gains sent to one room, in order, with their ramp_ms.
fn gains(effects: &[Effect], room: &str) -> Vec<(u16, u16)> {
    effects
        .iter()
        .filter_map(|e| match e {
            Effect::RoomVolume {
                zone,
                gain,
                ramp_ms,
                ..
            } if zone == room => Some((*gain, *ramp_ms)),
            _ => None,
        })
        .collect()
}

fn position(effects: &[Effect], wanted: impl Fn(&Effect) -> bool) -> usize {
    effects
        .iter()
        .position(wanted)
        .unwrap_or_else(|| panic!("not in {effects:#?}"))
}

fn set_source(group: &str, source: &str) -> impl Fn(&Effect) -> bool {
    let want = Effect::SetSource {
        group: group.into(),
        source: Source::parse(source).unwrap(),
    };
    move |e| *e == want
}

fn control(id: &str, action: InputAction) -> Effect {
    Effect::SourceControl {
        input: input(id),
        action,
    }
}

fn thursday(hour: i64, minute: i64, second: i64) -> i64 {
    utc(2026, 10, 1, hour, minute, second)
}

#[test]
fn alarm_with_a_chime_ramps_from_silence_to_its_volume_in_steps_and_ends_restored() {
    let mut h = House::new(Zone::utc(), thursday(6, 59, 0), &[("kitchen", 500)]);
    h.command(Command::AlarmSet(alarm(
        "wake",
        "kitchen",
        "07:00",
        "chime:bell",
        600,
        10,
    )));
    // Up to 07:00:00 and the fire.
    let fire = h.run(60_000, 100);
    let i0 = position(&fire, |e| {
        matches!(
            e,
            Effect::RoomVolume {
                gain: 0,
                ramp_ms: 0,
                ..
            }
        )
    });
    let is = position(&fire, set_source("kitchen", "chime:bell"));
    assert!(
        i0 < is,
        "silence goes out before the chime starts: {fire:#?}"
    );
    assert!(h.zones.is_ringing("wake"));
    assert_eq!(h.rt.last_fired("wake"), Some(thursday(7, 0, 0)));
    // The ramp: stepped once a second, never down, ending exactly at 600.
    let rise = h.run(15_000, 100);
    let steps = gains(&rise, "kitchen");
    assert!(steps.len() >= 10, "{steps:?}");
    assert!(steps.windows(2).all(|w| w[0].0 <= w[1].0), "{steps:?}");
    assert!(
        steps.iter().all(|(_, ms)| u64::from(*ms) <= STEP_MS),
        "{steps:?}"
    );
    assert!(
        steps
            .iter()
            .filter(|(_, ms)| u64::from(*ms) == STEP_MS)
            .count()
            >= 9
    );
    assert_eq!(steps.last().unwrap().0, 600);
    assert_eq!(h.volume("kitchen"), 600);
    assert_eq!(
        h.zones.zone("kitchen").unwrap().ramp,
        None,
        "the ramp is over"
    );
    // duration_min 1: at 07:01 it fades over two seconds, then the room is
    // put back as it was.
    let end = h.run(50_000, 100);
    let fade = gains(&end, "kitchen");
    let zero = fade.iter().position(|(g, _)| *g == 0).unwrap();
    assert!(
        fade[..=zero].windows(2).all(|w| w[0].0 >= w[1].0),
        "{fade:?}"
    );
    assert!(zero >= 1, "the fade is stepped: {fade:?}");
    let back = position(&end, set_source("kitchen", "stream"));
    let loud = position(&end, |e| matches!(e, Effect::RoomVolume { gain: 500, .. }));
    assert!(
        back < loud,
        "the chime is silenced before the volume comes back"
    );
    assert_eq!(h.volume("kitchen"), 500);
    assert_eq!(h.source_of("kitchen"), Source::Stream);
    assert!(!h.zones.is_ringing("wake") && h.rt.ringing().is_empty());
    assert!(!h.rt.is_held("kitchen"));
    assert!(end.contains(&Effect::Persist));
    assert_eq!(h.logged("alarm=wake ended restored=kitchen"), 1);
}

/// (goal 18) An alarm that rings while an announcement plays takes the room
/// from the announcement's player at the announcement's volume. The
/// announcement is over by the time the alarm ends, so the room goes back to
/// what it played and the volume it had BEFORE the announcement, never to
/// the announcement's player.
#[test]
fn an_alarm_that_displaced_an_announcement_puts_back_what_played_before_it() {
    let p0 = Source::Player("p0".to_string());
    let v = |t: i64| chorus_control::Volume::from_thousandths(t).unwrap();
    let mut h = House::new(Zone::utc(), thursday(6, 59, 0), &[("kitchen", 500)]);
    // The announcement: the kitchen played the stream at 0.500 and now plays
    // the announcement's player at 0.300.
    let announced = h
        .zones
        .announce_begin("kitchen", p0.clone(), Some(v(300)))
        .unwrap();
    assert_eq!(announced.previous, Source::Stream);
    assert_eq!(announced.volumes, [("kitchen".to_string(), v(500), v(300))]);
    h.command(Command::AlarmSet(alarm(
        "wake",
        "kitchen",
        "07:00",
        "chime:bell",
        600,
        10,
    )));
    h.run(60_000, 100);
    assert_eq!(h.source_of("kitchen"), Source::Chime("bell".to_string()));
    assert_eq!(h.zones.player_group("p0"), None, "the alarm took the room");
    // The conductor tells the runtime the announcement it displaced is over.
    assert_eq!(
        h.rt.announcement_over(&p0, &announced.previous, &announced.volumes),
        1
    );
    // A second telling changes nothing: no snapshot names the player now.
    assert_eq!(
        h.rt.announcement_over(&p0, &announced.previous, &announced.volumes),
        0
    );
    h.run(70_000, 100);
    assert!(!h.zones.is_ringing("wake") && h.rt.ringing().is_empty());
    assert_eq!(h.source_of("kitchen"), Source::Stream);
    assert_eq!(h.volume("kitchen"), 500);
}

/// (goal 16) A player plays in one group. A room that was playing one when
/// its alarm rang gets it back when the alarm ends, unless another group
/// took the player meanwhile: then the room plays nothing, and is not left
/// ringing.
#[test]
fn alarm_ending_gives_a_room_its_player_back_unless_another_group_took_it() {
    let p0 = || Source::Player("p0".to_string());
    for taken_meanwhile in [false, true] {
        let mut h = House::new(
            Zone::utc(),
            thursday(6, 59, 0),
            &[("kitchen", 500), ("study", 500)],
        );
        h.zones.set_group_source("kitchen", p0()).unwrap();
        h.command(Command::AlarmSet(alarm(
            "wake",
            "kitchen",
            "07:00",
            "chime:bell",
            600,
            10,
        )));
        h.run(60_000, 100);
        assert_eq!(h.source_of("kitchen"), Source::Chime("bell".to_string()));
        assert_eq!(h.zones.player_group("p0"), None, "the alarm freed it");
        if taken_meanwhile {
            h.zones.set_group_source("study", p0()).unwrap();
        }
        // duration_min 1: it ends at 07:01, fades, and the room is put back.
        h.run(70_000, 100);
        assert!(!h.zones.is_ringing("wake") && h.rt.ringing().is_empty());
        if taken_meanwhile {
            assert_eq!(h.source_of("study"), p0());
            assert_eq!(
                h.source_of("kitchen"),
                Source::None,
                "not given a player another group has, and not left ringing"
            );
            assert_eq!(h.logged("reason=player-busy wanted=player:p0"), 1);
        } else {
            assert_eq!(h.source_of("kitchen"), p0());
            assert_eq!(h.logged("reason=player-busy"), 0);
        }
    }
}

#[test]
fn alarm_with_a_line_in_source_starts_the_input_and_ramps() {
    let mut h = House::new(
        Zone::utc(),
        thursday(6, 59, 0),
        &[("bedroom", 300), ("lounge", 500)],
    );
    h.signal("turntable/line1", true);
    assert_eq!(h.zones.inputs(), &[input("turntable/line1")]);
    h.command(Command::AlarmSet(alarm(
        "radio",
        "bedroom",
        "07:00",
        "line-in:turntable/line1",
        400,
        5,
    )));
    let fire = h.run(60_000, 100);
    let is = position(&fire, set_source("bedroom", "line-in:turntable/line1"));
    let start = position(&fire, |e| {
        *e == control("turntable/line1", InputAction::Start)
    });
    assert!(is < start, "{fire:#?}");
    let rise = h.run(10_000, 100);
    let steps = gains(&rise, "bedroom");
    assert!(
        steps.len() >= 5 && steps.windows(2).all(|w| w[0].0 <= w[1].0),
        "{steps:?}"
    );
    assert_eq!(h.volume("bedroom"), 400);
    // alarm_stop: faded, the input stopped, the room restored.
    let out = h.command(Command::AlarmStop {
        alarm: "radio".into(),
    });
    assert!(gains(&out, "bedroom")[0].0 < 400, "the fade starts at once");
    let end = h.run(3_000, 100);
    let stop = position(&end, |e| {
        *e == control("turntable/line1", InputAction::Stop)
    });
    let back = position(&end, set_source("bedroom", "stream"));
    assert!(back < stop);
    assert_eq!(h.volume("bedroom"), 300);
    assert_eq!(h.source_of("bedroom"), Source::Stream);
    assert_eq!(
        h.volume("lounge"),
        500,
        "a room it never named is untouched"
    );
}

#[test]
fn alarm_with_a_line_in_not_offered_falls_back_to_a_chime() {
    let mut h = House::new(Zone::utc(), thursday(6, 59, 0), &[("bedroom", 300)]);
    h.command(Command::AlarmSet(alarm(
        "radio",
        "bedroom",
        "07:00",
        "line-in:turntable/line1",
        400,
        5,
    )));
    let fire = h.run(60_000, 100);
    position(&fire, set_source("bedroom", "chime:bell"));
    assert!(!fire
        .iter()
        .any(|e| matches!(e, Effect::SourceControl { .. })));
    assert_eq!(h.logged("alarm=radio fallback=chime reason=not-offered"), 1);
    h.run(10_000, 100);
    assert_eq!(h.volume("bedroom"), 400, "and it still ramps");
}

#[test]
fn alarm_with_a_line_in_whose_endpoint_goes_falls_back_to_the_chime_and_keeps_ringing() {
    let mut h = House::new(Zone::utc(), thursday(6, 59, 0), &[("bedroom", 300)]);
    h.signal("turntable/line1", true);
    h.command(Command::AlarmSet(alarm(
        "radio",
        "bedroom",
        "07:00",
        "line-in:turntable/line1",
        400,
        5,
    )));
    h.run(62_000, 100);
    let out = h.gone("turntable/line1");
    position(&out, set_source("bedroom", "chime:bell"));
    assert!(
        !out.iter()
            .any(|e| matches!(e, Effect::SourceControl { .. })),
        "nothing is sent to an endpoint that is gone"
    );
    assert!(h.zones.is_ringing("radio"));
    assert_eq!(h.logged("reason=input-gone"), 1);
}

#[test]
fn alarm_with_an_unknown_chime_name_plays_the_fallback_chime() {
    let mut h = House::new(Zone::utc(), thursday(6, 59, 0), &[("bedroom", 300)]);
    h.command(Command::AlarmSet(alarm(
        "odd",
        "bedroom",
        "07:00",
        "chime:gong",
        400,
        0,
    )));
    let fire = h.run(60_000, 100);
    position(&fire, set_source("bedroom", "chime:bell"));
    assert_eq!(h.logged("reason=unknown-chime"), 1);
    assert_eq!(h.volume("bedroom"), 400, "ramp_s 0 is the volume at once");
}

#[test]
fn alarm_across_the_spring_dst_change_fires_once_at_the_first_valid_instant() {
    // America/New_York, 2026-03-08: 02:00 EST becomes 03:00 EDT, so 02:30
    // does not happen and rings at 03:00 EDT (07:00 UTC); 01:30 EST is 06:30 UTC.
    let tz = fixture_zone("America_New_York");
    let start = utc(2026, 3, 8, 4, 0, 0); // 23:00 EST on the 7th
    let mut h = House::new(tz, start, &[("bedroom", 300)]);
    h.command(Command::AlarmSet(alarm(
        "gap",
        "bedroom",
        "02:30",
        "chime:bell",
        400,
        5,
    )));
    h.command(Command::AlarmSet(alarm(
        "plain",
        "bedroom",
        "01:30",
        "chime:triad",
        400,
        5,
    )));
    h.run(6 * 3_600_000, 10_000);
    assert_eq!(h.logged("alarm=gap fired"), 1);
    assert_eq!(h.logged("alarm=plain fired"), 1);
    assert_eq!(h.rt.last_fired("gap"), Some(utc(2026, 3, 8, 7, 0, 0)));
    assert_eq!(h.rt.last_fired("plain"), Some(utc(2026, 3, 8, 6, 30, 0)));
}

#[test]
fn alarm_across_the_autumn_dst_change_fires_once_at_the_first_of_the_repeated_hour() {
    // America/New_York, 2026-11-01: 02:00 EDT becomes 01:00 EST, so 01:30
    // happens twice (05:30 and 06:30 UTC) and rings once, at the first.
    let tz = fixture_zone("America_New_York");
    let start = utc(2026, 11, 1, 3, 0, 0); // 23:00 EDT on 31 October
    let mut h = House::new(tz, start, &[("bedroom", 300)]);
    h.command(Command::AlarmSet(alarm(
        "fold",
        "bedroom",
        "01:30",
        "chime:bell",
        400,
        5,
    )));
    h.command(Command::AlarmSet(alarm(
        "after",
        "bedroom",
        "02:30",
        "chime:bell",
        400,
        5,
    )));
    h.run(6 * 3_600_000, 10_000);
    assert_eq!(h.logged("alarm=fold fired"), 1);
    assert_eq!(h.rt.last_fired("fold"), Some(utc(2026, 11, 1, 5, 30, 0)));
    assert_eq!(h.logged("alarm=after fired"), 1);
    assert_eq!(h.rt.last_fired("after"), Some(utc(2026, 11, 1, 7, 30, 0)));
}

#[test]
fn alarm_one_shot_fires_once_and_disables() {
    let mut h = House::new(Zone::utc(), thursday(6, 59, 0), &[("bedroom", 300)]);
    let mut once = alarm("once", "bedroom", "07:00", "chime:bell", 400, 5);
    once.days = Days::NONE;
    h.command(Command::AlarmSet(once));
    let fire = h.run(60_000, 1_000);
    assert!(
        fire.contains(&Effect::Persist),
        "the disabled alarm is persisted"
    );
    assert!(!h.zones.alarms()[0].enabled);
    h.run(2 * 86_400_000, 60_000);
    assert_eq!(h.logged("alarm=once fired"), 1);
}

#[test]
fn alarm_does_not_refire_when_the_wall_clock_steps_back() {
    let mut h = House::new(Zone::utc(), thursday(6, 59, 0), &[("bedroom", 300)]);
    h.command(Command::AlarmSet(alarm(
        "wake",
        "bedroom",
        "07:00",
        "chime:bell",
        400,
        5,
    )));
    h.run(90_000, 1_000); // to 07:00:30
    assert_eq!(h.logged("alarm=wake fired"), 1);
    h.utc_base -= 120; // the civil clock steps back two minutes, during a 1 s tick
    h.run(300_000, 1_000); // through 07:00 again, and past the end
    assert_eq!(h.logged("stepped back by 119 s"), 1);
    assert_eq!(
        h.logged("alarm=wake fired"),
        1,
        "never twice for one instant"
    );
    // The next day's 07:00 is a new instant and rings.
    h.run(86_400_000, 30_000);
    assert_eq!(h.logged("alarm=wake fired"), 2);
}

#[test]
fn alarm_due_more_than_a_minute_ago_is_skipped_and_logged() {
    let mut h = House::new(Zone::utc(), thursday(6, 59, 0), &[("bedroom", 300)]);
    h.command(Command::AlarmSet(alarm(
        "wake",
        "bedroom",
        "07:00",
        "chime:bell",
        400,
        5,
    )));
    h.utc_base += 6 * 60; // the civil clock steps forward to 07:05
    h.run(1_000, 1_000);
    assert_eq!(h.logged("alarm=wake skipped late"), 1);
    assert_eq!(h.logged("alarm=wake fired"), 0);
    assert!(!h.zones.is_ringing("wake"));
    assert_eq!(h.volume("bedroom"), 300);
}

#[test]
fn quiet_window_start_pulls_volume_down_and_the_alarm_ramp_stops_at_the_quiet_cap() {
    let mut h = House::new(Zone::utc(), thursday(6, 29, 0), &[("kitchen", 800)]);
    h.command(Command::QuietHours {
        zone: "kitchen".into(),
        windows: vec![
            QuietWindow::from_persisted("mon,tue,wed,thu,fri,sat,sun 06:30-08:00 0.300").unwrap(),
        ],
    });
    // 06:30: the window starts; the gain ramps down first and the lower
    // limit follows one second later (ADR 0074).
    let pull = h.run(61_000, 100);
    let sent: Vec<&Effect> = pull
        .iter()
        .filter(|e| matches!(e, Effect::RoomVolume { .. }))
        .collect();
    assert_eq!(
        sent,
        [
            &Effect::RoomVolume {
                zone: "kitchen".into(),
                gain: 300,
                limit: 1000,
                ramp_ms: 1000
            },
            &Effect::RoomVolume {
                zone: "kitchen".into(),
                gain: 300,
                limit: 300,
                ramp_ms: 0
            },
        ]
    );
    assert_eq!(h.volume("kitchen"), 300);
    assert_eq!(
        h.logged("quiet-hours zone=kitchen effective_limit=0.300"),
        1
    );
    // An alarm at 07:00 to 0.600 ramps to the cap and no further.
    let mut wake = alarm("wake", "kitchen", "07:00", "chime:bell", 600, 20);
    wake.duration_min = 10;
    h.command(Command::AlarmSet(wake));
    let rise = h.run(31 * 60_000, 500);
    let steps = gains(&rise, "kitchen");
    assert!(steps.iter().all(|(g, _)| *g <= 300), "{steps:?}");
    assert_eq!(steps.iter().map(|(g, _)| *g).max(), Some(300));
    assert_eq!(h.volume("kitchen"), 300);
}

#[test]
fn quiet_window_starting_mid_ramp_holds_the_alarm_ramp_within_one_step() {
    let mut h = House::new(Zone::utc(), thursday(6, 59, 0), &[("kitchen", 200)]);
    h.command(Command::QuietHours {
        zone: "kitchen".into(),
        windows: vec![QuietWindow::from_persisted("thu 07:01-08:00 0.200").unwrap()],
    });
    let mut wake = alarm("wake", "kitchen", "07:00", "chime:bell", 800, 120);
    wake.duration_min = 10;
    h.command(Command::AlarmSet(wake));
    h.run(60_000, 100); // the fire at 07:00
    let before = h.run(59_000, 100);
    let top = gains(&before, "kitchen").last().unwrap().0;
    assert!(top > 300 && top < 800, "half way up: {top}");
    // 07:01: the window starts and the ramp holds at its cap from then on.
    let after = h.run(70_000, 100);
    let steps = gains(&after, "kitchen");
    // At most the one segment already under way when the window began; every
    // later report is at or under the cap.
    assert!(steps[1..].iter().all(|(g, _)| *g <= 200), "{steps:?}");
    assert_eq!(
        steps[1],
        (200, 1000),
        "pulled down over one segment: {steps:?}"
    );
    assert_eq!(h.volume("kitchen"), 200);
}

#[test]
fn alarm_ramp_tops_out_at_the_room_limit_and_restores_under_a_lowered_one() {
    let mut h = House::new(Zone::utc(), thursday(6, 59, 0), &[("kitchen", 800)]);
    h.command(Command::Limit {
        zone: "kitchen".into(),
        limit: vol(900),
    });
    h.command(Command::AlarmSet(alarm(
        "wake",
        "kitchen",
        "07:00",
        "chime:bell",
        1000,
        5,
    )));
    h.run(70_000, 100);
    assert_eq!(h.volume("kitchen"), 900, "1.000 asked for, 0.900 the limit");
    h.command(Command::Limit {
        zone: "kitchen".into(),
        limit: vol(400),
    });
    h.command(Command::AlarmStop {
        alarm: "wake".into(),
    });
    h.run(5_000, 100);
    assert_eq!(
        h.volume("kitchen"),
        400,
        "the 0.800 it had is clamped to the new limit"
    );
}

#[test]
fn alarm_ends_on_a_persons_command_naming_a_target_room_and_detaches_that_room() {
    let mut h = House::new(
        Zone::utc(),
        thursday(6, 59, 0),
        &[("kitchen", 500), ("bedroom", 300)],
    );
    h.command(Command::GroupSave {
        group: "upstairs".into(),
        name: "Upstairs".into(),
        zones: vec!["kitchen".into(), "bedroom".into()],
    });
    h.command(Command::AlarmSet(alarm(
        "wake",
        "upstairs",
        "07:00",
        "chime:bell",
        600,
        30,
    )));
    h.run(65_000, 100);
    assert_eq!(h.group("kitchen"), "upstairs");
    assert_eq!(h.group("bedroom"), "upstairs");
    // Mid-ramp, a person sets the kitchen's volume.
    h.command(Command::Volume {
        zone: "kitchen".into(),
        volume: vol(250),
    });
    assert!(!h.zones.is_ringing("wake"));
    let end = h.run(3_000, 100);
    assert!(
        gains(&end, "kitchen").is_empty(),
        "the kitchen is the person's now"
    );
    assert_eq!(h.volume("kitchen"), 250);
    assert!(!h.rt.is_held("kitchen") && !h.rt.is_held("bedroom"));
    // The bedroom faded and went back to its own group and its volume.
    assert_eq!(h.group("bedroom"), "bedroom");
    assert_eq!(h.volume("bedroom"), 300);
    assert_eq!(h.source_of("bedroom"), Source::Stream);
    // The kitchen stays where it is, the chime silenced.
    assert_eq!(h.group("kitchen"), "upstairs");
    assert_eq!(h.source_of("kitchen"), Source::None);
    assert_eq!(h.logged("reason=person"), 1);
}

#[test]
fn alarm_restores_a_live_group_it_broke_up() {
    let mut h = House::new(
        Zone::utc(),
        thursday(6, 59, 0),
        &[("kitchen", 500), ("lounge", 400)],
    );
    h.command(Command::Join {
        zone: "kitchen".into(),
        target: "lounge".into(),
    });
    let live = h.group("kitchen");
    assert_eq!(h.group("lounge"), live);
    h.command(Command::AlarmSet(alarm(
        "wake",
        "kitchen",
        "07:00",
        "chime:bell",
        600,
        5,
    )));
    h.run(65_000, 100);
    assert_eq!(h.group("kitchen"), "kitchen", "taken out");
    assert_eq!(
        h.group("lounge"),
        "lounge",
        "the live group left with one room dissolved"
    );
    h.run(60_000, 100); // duration_min 1, then the fade
    assert_eq!(h.group("kitchen"), h.group("lounge"), "grouped again");
    assert_eq!(h.volume("kitchen"), 500);
}

#[test]
fn sleep_timer_fades_to_silence_then_stops_and_restores_the_volume() {
    let mut h = House::new(Zone::utc(), thursday(22, 0, 0), &[("bedroom", 600)]);
    h.command(Command::Sleep {
        target: "bedroom".into(),
        minutes: 1,
    });
    assert!(h.rt.is_sleeping("bedroom"));
    let wait = h.run(29_000, 100);
    assert!(
        gains(&wait, "bedroom").is_empty(),
        "nothing before the fade"
    );
    let fade = h.run(30_900, 100);
    let steps = gains(&fade, "bedroom");
    assert!(steps.len() >= 30, "stepped once a second: {steps:?}");
    assert!(steps.windows(2).all(|w| w[0].0 >= w[1].0), "{steps:?}");
    assert_eq!(steps.last().unwrap().0, 0);
    assert_eq!(
        h.source_of("bedroom"),
        Source::Stream,
        "still playing until expiry"
    );
    let stop = h.run(200, 100);
    let silent = position(&stop, set_source("bedroom", "none"));
    let back = position(&stop, |e| {
        matches!(
            e,
            Effect::RoomVolume {
                gain: 600,
                ramp_ms: 0,
                ..
            }
        )
    });
    assert!(silent < back, "stopped before the volume comes back");
    assert_eq!(h.volume("bedroom"), 600);
    assert!(h.zones.sleep_timers().is_empty() && !h.rt.is_sleeping("bedroom"));
    assert!(stop.contains(&Effect::Persist));
}

#[test]
fn sleep_timer_is_cancelled_by_a_persons_volume_change_during_the_fade() {
    let mut h = House::new(
        Zone::utc(),
        thursday(22, 0, 0),
        &[("kitchen", 500), ("bedroom", 600)],
    );
    h.command(Command::Join {
        zone: "kitchen".into(),
        target: "bedroom".into(),
    });
    let group = h.group("bedroom");
    h.command(Command::Sleep {
        target: group.clone(),
        minutes: 1,
    });
    h.run(45_000, 100); // fifteen seconds into the fade
    assert!(h.volume("bedroom") < 600 && h.volume("kitchen") < 500);
    let out = h.command(Command::Volume {
        zone: "kitchen".into(),
        volume: vol(350),
    });
    assert!(!h.rt.is_sleeping(&group) && h.zones.sleep_timers().is_empty());
    assert_eq!(
        gains(&out, "bedroom"),
        [(600, STEP_MS as u16)],
        "the room the person did not touch comes back over one step"
    );
    h.run(60_000, 100);
    assert_eq!(h.volume("kitchen"), 350);
    assert_eq!(h.volume("bedroom"), 600);
    assert_eq!(h.source_of("bedroom"), Source::Stream, "it never stopped");
    assert_eq!(h.logged("cancelled reason=volume-changed"), 1);
}

#[test]
fn sleep_timer_zero_minutes_cancels_it() {
    let mut h = House::new(Zone::utc(), thursday(22, 0, 0), &[("bedroom", 600)]);
    h.command(Command::Sleep {
        target: "bedroom".into(),
        minutes: 1,
    });
    h.run(10_000, 100);
    h.command(Command::Sleep {
        target: "bedroom".into(),
        minutes: 0,
    });
    let after = h.run(120_000, 1_000);
    assert!(gains(&after, "bedroom").is_empty());
    assert_eq!(h.source_of("bedroom"), Source::Stream);
    assert!(!h.rt.is_sleeping("bedroom"));
}

fn autoplay_house(target: &str) -> House {
    let mut h = House::new(
        Zone::utc(),
        thursday(18, 0, 0),
        &[("kitchen", 500), ("lounge", 400)],
    );
    h.command(Command::GroupSave {
        group: "downstairs".into(),
        name: "Downstairs".into(),
        zones: vec!["kitchen".into(), "lounge".into()],
    });
    h.command(Command::Autoplay(Autoplay {
        input: input("turntable/line1"),
        target: target.into(),
        enabled: true,
        stop_on_standby: true,
        low_latency: true,
    }));
    h.signal("turntable/line1", false); // offered, no signal yet
    h
}

// --- goal 13: TV autoplay (K81) ------------------------------------------

/// The house of a TV: the lounge (the theater room) and the kitchen grouped
/// as "downstairs" and playing the stream, and an autoplay rule for the hub's
/// TV input (an `hdmi_arc` or `optical` input; the runtime is told a standby
/// only for those, `crate::linein`) that targets the lounge alone.
fn tv_house(stop_on_standby: bool) -> House {
    let mut h = House::new(
        Zone::utc(),
        thursday(20, 0, 0),
        &[("kitchen", 500), ("lounge", 400)],
    );
    h.command(Command::GroupSave {
        group: "downstairs".into(),
        name: "Downstairs".into(),
        zones: vec!["kitchen".into(), "lounge".into()],
    });
    h.command(Command::Take {
        target: "downstairs".into(),
        source: Some(Source::Stream),
    });
    h.command(Command::Autoplay(Autoplay {
        input: input("hub/tv"),
        target: "lounge".into(),
        enabled: true,
        stop_on_standby,
        low_latency: true,
    }));
    h.signal("hub/tv", false); // offered, the TV off
    assert_eq!(h.group("lounge"), "downstairs");
    h
}

impl House {
    fn standby(&mut self, id: &str) -> Vec<Effect> {
        let out = self
            .rt
            .on_input_standby(&input(id), self.mono, &mut self.zones);
        self.check(out)
    }
}

#[test]
fn tv_on_takes_the_theater_room_out_of_its_group_and_plays_the_tv() {
    let mut h = tv_house(true);
    let on = h.signal("hub/tv", true);
    // The lounge leaves "downstairs" and plays the TV; the kitchen stays.
    assert_eq!(h.group("lounge"), "lounge");
    assert_eq!(h.source_of("lounge"), Source::LineIn(input("hub/tv")));
    assert_eq!(h.group("kitchen"), "downstairs");
    assert_eq!(h.source_of("kitchen"), Source::Stream);
    assert!(on.contains(&control("hub/tv", InputAction::Start)));
    assert!(h.rt.is_autoplaying(&input("hub/tv")));
    assert!(h.rt.is_held("lounge"));
}

#[test]
fn tv_standby_stops_it_at_once_and_restores_what_the_room_played() {
    let mut h = tv_house(true);
    h.signal("hub/tv", true);
    let off = h.standby("hub/tv");
    // At once: no hold, the TV's input stopped in the same call.
    assert!(off.contains(&control("hub/tv", InputAction::Stop)));
    assert!(!h.rt.is_autoplaying(&input("hub/tv")));
    assert!(!h.rt.is_held("lounge"));
    // Restored: back in "downstairs", playing the stream, at its volume.
    assert_eq!(h.group("lounge"), "downstairs");
    assert_eq!(h.source_of("lounge"), Source::Stream);
    assert_eq!(h.volume("lounge"), 400);
    assert_eq!(h.logged("stopped reason=standby"), 1);
    assert!(h.zones.inputs().is_empty(), "withdrawn");
    // Nothing is left to stop later.
    let later = h.run(2 * AUTOPLAY_HOLD_MS, 1_000);
    assert!(!later
        .iter()
        .any(|e| matches!(e, Effect::SourceControl { .. })));
}

#[test]
fn tv_standby_with_stop_on_standby_off_holds_as_a_signal_gone() {
    let mut h = tv_house(false);
    h.signal("hub/tv", true);
    let off = h.standby("hub/tv");
    assert!(!off.contains(&control("hub/tv", InputAction::Stop)));
    assert!(h.rt.is_autoplaying(&input("hub/tv")));
    h.run(AUTOPLAY_HOLD_MS - 1_000, 500);
    assert_eq!(h.source_of("lounge"), Source::LineIn(input("hub/tv")));
    let over = h.run(1_000, 500);
    assert!(over.contains(&control("hub/tv", InputAction::Stop)));
    assert_eq!(h.group("lounge"), "downstairs");
    assert_eq!(h.logged("stopped reason=hold-over"), 1);
}

#[test]
fn a_non_tv_line_in_keeps_its_hold_and_a_standby_with_nothing_playing_is_a_signal_gone() {
    // A turntable's signal going is the 30 s hold, as before goal 13: the
    // runtime is never told a standby for it (only a TV input's offer is
    // one, crates/server/src/linein.rs), and its rule's default changes
    // nothing.
    let mut h = autoplay_house("lounge");
    h.signal("turntable/line1", true);
    let off = h.signal("turntable/line1", false);
    assert!(!off.contains(&control("turntable/line1", InputAction::Stop)));
    assert_eq!(h.logged("holding 30000 ms"), 1);
    h.run(AUTOPLAY_HOLD_MS - 1_000, 500);
    assert!(h.rt.is_autoplaying(&input("turntable/line1")));
    // A TV that goes to standby while nothing plays it: just withdrawn.
    let mut t = tv_house(true);
    let out = t.standby("hub/tv");
    assert!(!out
        .iter()
        .any(|e| matches!(e, Effect::SourceControl { .. })));
    assert_eq!(t.group("lounge"), "downstairs");
}

#[test]
fn line_in_autoplay_takes_the_target_on_signal_then_stops_and_restores_after_the_hold() {
    let mut h = autoplay_house("lounge");
    let on = h.signal("turntable/line1", true);
    let is = position(&on, set_source("lounge", "line-in:turntable/line1"));
    let start = position(&on, |e| {
        *e == control("turntable/line1", InputAction::Start)
    });
    assert!(is < start);
    assert!(h.rt.is_autoplaying(&input("turntable/line1")));
    assert_eq!(h.volume("lounge"), 400, "autoplay keeps the room's volume");
    assert_eq!(h.source_of("kitchen"), Source::Stream, "only its target");
    // The signal goes: it holds, then stops and restores.
    h.signal("turntable/line1", false);
    let held = h.run(AUTOPLAY_HOLD_MS - 1_000, 500);
    assert!(!held.contains(&control("turntable/line1", InputAction::Stop)));
    assert_eq!(
        h.source_of("lounge"),
        Source::LineIn(input("turntable/line1"))
    );
    let over = h.run(1_000, 500);
    let back = position(&over, set_source("lounge", "stream"));
    let stop = position(&over, |e| {
        *e == control("turntable/line1", InputAction::Stop)
    });
    assert!(back < stop);
    assert!(!h.rt.is_autoplaying(&input("turntable/line1")));
    assert!(!h.rt.is_held("lounge"));
    assert_eq!(h.volume("lounge"), 400);
}

#[test]
fn line_in_autoplay_signal_back_within_the_hold_cancels_the_stop() {
    let mut h = autoplay_house("lounge");
    h.signal("turntable/line1", true);
    h.signal("turntable/line1", false);
    h.run(10_000, 500);
    h.signal("turntable/line1", true);
    let later = h.run(2 * AUTOPLAY_HOLD_MS, 500);
    assert!(!later
        .iter()
        .any(|e| matches!(e, Effect::SourceControl { .. })));
    assert_eq!(
        h.source_of("lounge"),
        Source::LineIn(input("turntable/line1"))
    );
    assert_eq!(h.logged("stop cancelled"), 1);
}

#[test]
fn line_in_autoplay_a_persons_command_on_a_target_room_detaches_it() {
    let mut h = autoplay_house("downstairs");
    h.signal("turntable/line1", true);
    assert_eq!(h.group("kitchen"), "downstairs");
    assert_eq!(h.group("lounge"), "downstairs");
    // A level is not a source change (goal 13): the kitchen turned up and
    // the lounge muted are still the autoplay's.
    h.command(Command::Volume {
        zone: "kitchen".into(),
        volume: vol(700),
    });
    h.command(Command::Mute {
        zone: "lounge".into(),
        muted: true,
    });
    assert!(h.rt.is_held("kitchen") && h.rt.is_held("lounge"));
    assert_eq!(h.logged("detached"), 0);
    // The kitchen taken out of the group is a person choosing something
    // else: it is let go.
    h.command(Command::Ungroup {
        zone: "kitchen".into(),
    });
    assert!(!h.rt.is_held("kitchen") && h.rt.is_held("lounge"));
    h.signal("turntable/line1", false);
    h.run(AUTOPLAY_HOLD_MS + 1_000, 500);
    // The lounge is put back, its mute too; the kitchen stays as the person
    // left it.
    assert_eq!(h.group("lounge"), "lounge");
    assert_eq!(h.source_of("lounge"), Source::Stream);
    assert_eq!(h.volume("lounge"), 400);
    assert!(!h.zones.zone("lounge").unwrap().muted);
    assert_eq!(h.group("kitchen"), "kitchen");
    assert_eq!(h.volume("kitchen"), 700);
    assert_eq!(h.logged("zone=kitchen detached"), 1);
}

#[test]
fn tv_volume_keys_and_mute_while_the_tv_plays_keep_the_autoplay_and_standby_restores() {
    // The hub's CEC keys reach the server as `volume_step` and `mute` on the
    // lounge (ADR 0087); a group volume on the autoplay's own group is a
    // level too.
    let mut h = tv_house(true);
    h.signal("hub/tv", true);
    for _ in 0..3 {
        h.command(Command::VolumeStep {
            zone: "lounge".into(),
            step: 20,
        });
    }
    assert_eq!(h.volume("lounge"), 460);
    h.command(Command::Mute {
        zone: "lounge".into(),
        muted: true,
    });
    h.command(Command::GroupVolumeStep {
        group: "lounge".into(),
        step: -20,
    });
    assert_eq!(h.volume("lounge"), 440);
    assert!(h.rt.is_held("lounge"));
    assert!(h.rt.is_autoplaying(&input("hub/tv")));
    assert_eq!(h.logged("detached"), 0);
    // The standby still stops it at once and puts the lounge back as it was
    // before the TV: in "downstairs", the stream, 0.400, not muted.
    let off = h.standby("hub/tv");
    assert!(off.contains(&control("hub/tv", InputAction::Stop)));
    assert!(!h.rt.is_held("lounge"));
    assert_eq!(h.group("lounge"), "downstairs");
    assert_eq!(h.source_of("lounge"), Source::Stream);
    assert_eq!(h.volume("lounge"), 400);
    assert!(!h.zones.zone("lounge").unwrap().muted);
    assert_eq!(h.logged("stopped reason=standby restored=lounge"), 1);
}

#[test]
fn tv_a_person_choosing_another_source_still_detaches_the_room() {
    let mut h = tv_house(true);
    h.signal("hub/tv", true);
    h.command(Command::Join {
        zone: "lounge".into(),
        target: "downstairs".into(),
    });
    assert!(!h.rt.is_held("lounge"));
    assert_eq!(h.logged("zone=lounge detached"), 1);
    // The standby then has nothing of the lounge's to restore.
    h.standby("hub/tv");
    assert_eq!(h.group("lounge"), "downstairs");
    assert_eq!(h.source_of("lounge"), Source::Stream);
}

#[test]
fn alarm_still_ends_on_a_persons_mute_naming_its_room() {
    // ADR 0076's rule is unchanged by goal 13's autoplay exemption: any
    // person's command naming a ringing alarm's room ends the alarm.
    let mut h = House::new(Zone::utc(), thursday(6, 59, 0), &[("bedroom", 300)]);
    h.command(Command::AlarmSet(alarm(
        "wake",
        "bedroom",
        "07:00",
        "chime:bell",
        600,
        30,
    )));
    h.run(65_000, 100);
    assert!(h.zones.is_ringing("wake"));
    h.command(Command::Mute {
        zone: "bedroom".into(),
        muted: true,
    });
    assert!(!h.zones.is_ringing("wake"));
    assert_eq!(h.logged("reason=person"), 1);
}

#[test]
fn line_in_autoplay_whose_endpoint_goes_stops_at_once_and_restores() {
    let mut h = autoplay_house("lounge");
    h.signal("turntable/line1", true);
    let out = h.gone("turntable/line1");
    position(&out, set_source("lounge", "stream"));
    assert!(!out
        .iter()
        .any(|e| matches!(e, Effect::SourceControl { .. })));
    assert!(!h.rt.is_autoplaying(&input("turntable/line1")));
    assert!(h.zones.inputs().is_empty());
}

/// (goal 17) An input plays in any number of groups: one a person already
/// plays in the kitchen is autoplayed into its target too, and started once.
#[test]
fn line_in_autoplay_shares_an_input_another_group_already_plays() {
    let mut h = autoplay_house("lounge");
    h.command(Command::Take {
        target: "kitchen".into(),
        source: Some(Source::LineIn(input("turntable/line1"))),
    });
    let on = h.signal("turntable/line1", true);
    assert!(h.rt.is_autoplaying(&input("turntable/line1")));
    let line_in = Source::LineIn(input("turntable/line1"));
    assert_eq!(h.source_of("lounge"), line_in);
    assert_eq!(h.source_of("kitchen"), line_in);
    assert_eq!(
        on.iter()
            .filter(|e| **e == control("turntable/line1", InputAction::Start))
            .count(),
        0,
        "started once already, for the kitchen"
    );
    // The signal goes and the hold runs out: the target is restored, and
    // the kitchen, which a person chose, keeps the input (still started).
    h.signal("turntable/line1", false);
    let end = h.run(AUTOPLAY_HOLD_MS + 1_000, 500);
    assert_eq!(h.source_of("lounge"), Source::Stream);
    assert_eq!(h.source_of("kitchen"), line_in);
    assert!(!end.contains(&control("turntable/line1", InputAction::Stop)));
}

/// (goal 17) A target that already plays the input, exactly as the take
/// would leave it, is left as the person made it: nothing held or restored.
#[test]
fn line_in_autoplay_leaves_a_target_that_already_plays_the_input() {
    let mut h = autoplay_house("lounge");
    h.command(Command::Take {
        target: "lounge".into(),
        source: Some(Source::LineIn(input("turntable/line1"))),
    });
    h.signal("turntable/line1", true);
    assert!(!h.rt.is_autoplaying(&input("turntable/line1")));
    assert!(!h.rt.is_held("lounge"));
    assert_eq!(h.logged("already plays in its target=lounge"), 1);
}

/// (goal 17) An alarm whose line-in another group already plays plays it
/// too: there is no `input-busy` fallback any more.
#[test]
fn alarm_with_a_line_in_another_group_plays_plays_it_too() {
    let mut h = House::new(
        Zone::utc(),
        thursday(6, 59, 0),
        &[("bedroom", 300), ("lounge", 500)],
    );
    h.signal("turntable/line1", true);
    let started = h.command(Command::Take {
        target: "lounge".into(),
        source: Some(Source::LineIn(input("turntable/line1"))),
    });
    assert!(started.contains(&control("turntable/line1", InputAction::Start)));
    h.command(Command::AlarmSet(alarm(
        "radio",
        "bedroom",
        "07:00",
        "line-in:turntable/line1",
        400,
        5,
    )));
    let fire = h.run(60_000, 100);
    position(&fire, set_source("bedroom", "line-in:turntable/line1"));
    assert_eq!(h.logged("fallback=chime"), 0);
    assert_eq!(h.logged("input-busy"), 0);
    assert!(
        !fire.contains(&control("turntable/line1", InputAction::Start)),
        "one start serves both groups"
    );
    // The alarm ends: the bedroom is restored, the lounge keeps the input.
    h.command(Command::AlarmStop {
        alarm: "radio".into(),
    });
    let end = h.run(3_000, 100);
    assert_eq!(h.source_of("bedroom"), Source::Stream);
    assert_eq!(
        h.source_of("lounge"),
        Source::LineIn(input("turntable/line1"))
    );
    assert!(!end.contains(&control("turntable/line1", InputAction::Stop)));
}

fn store(h: &mut House, id: &str, kind: StoredKind, value: &str, name: &str) {
    h.command(Command::SourceStore(StoredSource {
        id: id.into(),
        kind,
        value: value.into(),
        name: name.into(),
    }));
}

fn stored_house() -> House {
    let mut h = House::new(
        Zone::utc(),
        thursday(6, 59, 0),
        &[("bedroom", 300), ("lounge", 500)],
    );
    store(
        &mut h,
        "radio",
        StoredKind::Url,
        "https://radio.example/stream.mp3",
        "Morning radio",
    );
    h.command(Command::AlarmSet(alarm(
        "wake",
        "bedroom",
        "07:00",
        "stored:radio",
        400,
        5,
    )));
    h
}

fn play_stored() -> Effect {
    Effect::PlayStored {
        alarm: "wake".into(),
        target: "bedroom".into(),
        stored: "radio".into(),
        url: "https://radio.example/stream.mp3".into(),
        name: "Morning radio".into(),
    }
}

/// (goal 17) An alarm whose source is a stored stream URL fires in silence,
/// asks for the play once, and plays the player it is told started; stop
/// fades, restores, and gives the player back.
#[test]
fn alarm_with_a_stored_url_asks_for_the_play_and_plays_the_player_it_is_given() {
    let mut h = stored_house();
    let fire = h.run(60_000, 100);
    assert_eq!(
        fire.iter().filter(|e| **e == play_stored()).count(),
        1,
        "{fire:#?}"
    );
    position(&fire, set_source("bedroom", "none"));
    assert!(h.zones.is_ringing("wake"));
    assert_eq!(h.rt.alarm_stored("wake"), Some("radio"));
    // The conductor's answer: the player plays.
    let p0 = Source::Player("p0".into());
    let (mono, _) = (h.mono, ());
    let (took, out) =
        h.rt.on_alarm_source_started("wake", p0.clone(), mono, &mut h.zones);
    let out = h.check(out);
    assert!(took);
    position(&out, set_source("bedroom", "player:p0"));
    assert_eq!(h.source_of("bedroom"), p0);
    assert_eq!(
        h.logged("alarm=wake started stored=radio plays=player:p0"),
        1
    );
    // A second answer is not taken: the alarm no longer waits.
    let (again, _) =
        h.rt.on_alarm_source_started("wake", Source::Player("p1".into()), mono, &mut h.zones);
    assert!(!again);
    assert_eq!(h.source_of("bedroom"), p0);
    // The ramp runs as for any source.
    h.run(10_000, 100);
    assert_eq!(h.volume("bedroom"), 400);
    // Stop: the fade, then the restore, and the player is given back.
    h.command(Command::AlarmStop {
        alarm: "wake".into(),
    });
    let end = h.run(3_000, 100);
    position(&end, |e| {
        *e == Effect::StopStored {
            alarm: "wake".into(),
        }
    });
    assert_eq!(h.source_of("bedroom"), Source::Stream);
    assert_eq!(h.volume("bedroom"), 300);
    assert!(h.rt.ringing().is_empty());
    assert_eq!(h.logged("fallback=chime"), 0);
}

/// (goal 17) Whatever stops the stream, at once or later, rings the
/// fallback chime with the reason it was given, and the alarm keeps ringing.
#[test]
fn alarm_with_a_stored_url_that_fails_rings_the_chime_with_the_reason() {
    for (after_start, reason, detail) in [
        (
            false,
            "no-players",
            "this server was started without --players",
        ),
        (
            false,
            "no-free-player",
            "players: no free player: all 1 are in use",
        ),
        (true, "url-refused", "refused: loopback address 127.0.0.1"),
        (true, "stream-failed", "http status 404"),
        (true, "stream-ended", "the stream ended"),
    ] {
        let mut h = stored_house();
        h.run(60_000, 100);
        let mono = h.mono;
        if after_start {
            let (took, out) = h.rt.on_alarm_source_started(
                "wake",
                Source::Player("p0".into()),
                mono,
                &mut h.zones,
            );
            assert!(took);
            h.check(out);
        }
        let out =
            h.rt.on_alarm_source_failed("wake", reason, detail, mono, &mut h.zones);
        let out = h.check(out);
        position(&out, set_source("bedroom", "chime:bell"));
        assert_eq!(
            h.logged(&format!(
                "alarm=wake fallback=chime reason={} wanted=stored:radio plays=chime:bell \
                 detail=\"{}\"",
                reason, detail
            )),
            1,
            "{reason}"
        );
        assert!(
            h.zones.is_ringing("wake"),
            "{reason}: an alarm must still wake"
        );
        assert_eq!(h.rt.alarm_stored("wake"), None);
        // A second failure report changes nothing.
        let again =
            h.rt.on_alarm_source_failed("wake", reason, detail, mono, &mut h.zones);
        assert!(again.is_empty(), "{again:#?}");
        h.run(10_000, 100);
        assert_eq!(h.volume("bedroom"), 400, "{reason}: and it still ramps");
        // It ends as a chime alarm does.
        h.command(Command::AlarmStop {
            alarm: "wake".into(),
        });
        let end = h.run(3_000, 100);
        assert_eq!(h.source_of("bedroom"), Source::Stream);
        assert!(
            !end.iter().any(|e| matches!(e, Effect::StopStored { .. })),
            "{reason}: nothing is held any more"
        );
    }
}

/// (goal 17) An answer for an alarm that already ended is not taken, so the
/// caller gives the player back.
#[test]
fn a_stored_url_that_starts_after_the_alarm_was_stopped_is_not_taken() {
    let mut h = stored_house();
    h.run(60_000, 100);
    h.command(Command::AlarmStop {
        alarm: "wake".into(),
    });
    h.run(3_000, 100);
    let mono = h.mono;
    let (took, _) =
        h.rt.on_alarm_source_started("wake", Source::Player("p0".into()), mono, &mut h.zones);
    assert!(!took);
    assert_eq!(h.source_of("bedroom"), Source::Stream);
}

/// (goal 17) The Spotify alarm source ships switched off: the chime rings
/// with reason `soloist-off`. Switched on (the Soloist server track's flag),
/// the alarm asks with `PlaySpotify` and takes the same two answers.
#[test]
fn a_stored_spotify_alarm_rings_the_chime_until_soloist_alarms_are_switched_on() {
    let spotify = |on: bool| {
        let mut h = House::new(Zone::utc(), thursday(6, 59, 0), &[("bedroom", 300)]);
        h.rt.set_soloist_alarms(on);
        store(
            &mut h,
            "wake-list",
            StoredKind::Spotify,
            "spotify:playlist:37i9dQZF1DXexample0000",
            "Wake up",
        );
        h.command(Command::AlarmSet(alarm(
            "wake",
            "bedroom",
            "07:00",
            "stored:wake-list",
            400,
            5,
        )));
        let fire = h.run(60_000, 100);
        (h, fire)
    };
    let (h, fire) = spotify(false);
    position(&fire, set_source("bedroom", "chime:bell"));
    assert_eq!(
        h.logged("alarm=wake fallback=chime reason=soloist-off wanted=stored:wake-list"),
        1
    );
    assert!(!fire.iter().any(|e| matches!(e, Effect::PlaySpotify { .. })));

    let (mut h, fire) = spotify(true);
    position(&fire, |e| {
        *e == Effect::PlaySpotify {
            alarm: "wake".into(),
            target: "bedroom".into(),
            stored: "wake-list".into(),
            uri: "spotify:playlist:37i9dQZF1DXexample0000".into(),
            name: "Wake up".into(),
        }
    });
    assert_eq!(h.source_of("bedroom"), Source::None);
    let mono = h.mono;
    let out = h.rt.on_alarm_source_failed(
        "wake",
        "soloist-unavailable",
        "this server runs no Soloist receiver",
        mono,
        &mut h.zones,
    );
    position(&out, set_source("bedroom", "chime:bell"));
    assert!(h.zones.is_ringing("wake"));
}

/// (goal 17) A `streamer` input plays into its endpoint's own room when its
/// signal appears, with no autoplay rule; a plain line-in does not; a
/// disabled rule switches the streamer's default off.
#[test]
fn a_streamer_input_autoplays_into_its_endpoints_room() {
    let label = |role: InputRole| {
        Command::InputLabel(InputLabel {
            input: input("amp/line1"),
            name: "Lounge streamer".into(),
            role,
        })
    };
    let house = || {
        let mut h = House::new(
            Zone::utc(),
            thursday(18, 0, 0),
            &[("kitchen", 500), ("lounge", 400)],
        );
        h.command(Command::Attach {
            zone: "lounge".into(),
            endpoint: "amp".into(),
            link: None,
        });
        h
    };
    let line_in = Source::LineIn(input("amp/line1"));

    let mut h = house();
    h.command(label(InputRole::Streamer));
    h.signal("amp/line1", true);
    assert!(h.rt.is_autoplaying(&input("amp/line1")));
    assert_eq!(h.source_of("lounge"), line_in);
    let playing = h.zones.now_playing("lounge").expect("the label is shown");
    assert_eq!(playing.title.as_deref(), Some("Lounge streamer"));
    assert_eq!(playing.via, "streamer");
    // Shared to a second group like any line-in, with the label.
    h.command(Command::Take {
        target: "kitchen".into(),
        source: Some(line_in.clone()),
    });
    assert_eq!(
        h.zones.now_playing("kitchen").and_then(|p| p.title.clone()),
        Some("Lounge streamer".to_string())
    );
    // The signal goes: after the hold the room is restored, record and all.
    h.signal("amp/line1", false);
    h.run(AUTOPLAY_HOLD_MS + 1_000, 500);
    assert_eq!(h.source_of("lounge"), Source::Stream);
    assert!(h.zones.now_playing("lounge").is_none());

    let mut h = house();
    h.command(label(InputRole::LineIn));
    h.signal("amp/line1", true);
    assert!(!h.rt.is_autoplaying(&input("amp/line1")));
    assert_eq!(h.source_of("lounge"), Source::Stream);
    // Labelled a streamer while its signal is present: it plays now.
    h.command(label(InputRole::Streamer));
    assert!(h.rt.is_autoplaying(&input("amp/line1")));
    assert_eq!(h.source_of("lounge"), line_in);

    let mut h = house();
    h.command(label(InputRole::Streamer));
    h.command(Command::Autoplay(Autoplay {
        input: input("amp/line1"),
        target: "lounge".into(),
        enabled: false,
        stop_on_standby: true,
        low_latency: true,
    }));
    h.signal("amp/line1", true);
    assert!(!h.rt.is_autoplaying(&input("amp/line1")));
}

#[test]
fn alarm_taking_a_room_autoplay_holds_restores_it_to_what_it_was_before_either() {
    let mut h = autoplay_house("lounge");
    h.signal("turntable/line1", true);
    h.command(Command::AlarmSet(alarm(
        "wake",
        "lounge",
        "18:01",
        "chime:bell",
        600,
        5,
    )));
    let fire = h.run(61_000, 500);
    position(&fire, set_source("lounge", "chime:bell"));
    assert!(fire.contains(&control("turntable/line1", InputAction::Stop)));
    assert!(!h.rt.is_autoplaying(&input("turntable/line1")));
    h.run(65_000, 500);
    assert_eq!(
        h.source_of("lounge"),
        Source::Stream,
        "the stream it had before the autoplay"
    );
    assert_eq!(h.volume("lounge"), 400);
}

#[test]
fn next_deadline_names_the_next_ramp_step() {
    let mut h = House::new(Zone::utc(), thursday(6, 59, 59), &[("kitchen", 500)]);
    h.command(Command::AlarmSet(alarm(
        "wake",
        "kitchen",
        "07:00",
        "chime:bell",
        600,
        10,
    )));
    assert_eq!(h.rt.next_deadline_ns(), None);
    h.run(1_000, 1_000);
    let fired_at = h.mono;
    assert_eq!(h.rt.next_deadline_ns(), Some(fired_at));
    h.tick();
    assert_eq!(h.rt.next_deadline_ns(), Some(fired_at + STEP_MS * MS));
}

#[test]
fn the_schedule_runtime_reads_no_clock_and_opens_nothing() {
    // Pure by construction: time is passed in. The settable spellings are
    // the audio-path check's own; the rest are the monotonic clock, threads
    // and sockets, none of which this module may touch.
    let path = format!("{}/src/schedule_runtime.rs", env!("CARGO_MANIFEST_DIR"));
    let text = std::fs::read_to_string(path).unwrap();
    for (n, line) in text.lines().enumerate() {
        let code = line.split("//").next().unwrap_or("");
        assert_eq!(
            chorus_audio_path::scan::settable_clock_in(code),
            None,
            "line {}: {}",
            n + 1,
            line
        );
        for word in [
            "Instant",
            "std::time",
            "clock_gettime",
            "std::thread",
            "std::net",
            "std::fs",
        ] {
            assert!(!code.contains(word), "line {}: {}", n + 1, line);
        }
    }
    // The check sees a clock when there is one.
    assert!(chorus_audio_path::scan::settable_clock_in("std::time::SystemTime::now()").is_some());
}
