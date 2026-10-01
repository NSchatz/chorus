//! The Rust twin held to the C model's committed fixtures.
//!
//! Every script here is `firmware/tests/test_controls.c`'s, input for input
//! and millisecond for millisecond, on a fake monotonic clock polled every
//! 1 ms. The `controller_command` frames the Rust controls produce must equal
//! `fixtures/controls/<class>.hex` byte for byte, and the LED fed
//! `visualizer-sequence.hex` must show `visualizer-sequence.led` at every
//! moment. The fixtures are committed and never written by a test: if this
//! disagrees with them, the Rust is wrong (conventions rule 9).

use std::path::PathBuf;

use chorus_controls::model::{KNOB_MAX, MAX_ACTIONS};
use chorus_controls::{Action, ActionKind, Controls, Input, Led, LedInputs, LedKind, LedState};
use chorus_controls::{LedOutput, SpeakerClass};
use chorus_protocol::v2::{self, ControllerState, Message, Outcome, Playback};

const MS: u64 = 1_000_000;

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/controls/")
        .join(name)
}

fn fixture(name: &str) -> String {
    let path = fixture_path(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn parse_hex(text: &str) -> Vec<u8> {
    text.lines()
        .map(|l| l.split('#').next().unwrap_or(""))
        .flat_map(|l| l.split_whitespace())
        .map(|b| u8::from_str_radix(b, 16).expect("a hex byte"))
        .collect()
}

/// What the controls produced: the frames, concatenated, and every action.
#[derive(Default)]
struct Sink {
    bytes: Vec<u8>,
    actions: Vec<Action>,
}

impl Sink {
    fn count(&self, f: impl Fn(&ActionKind) -> bool) -> usize {
        self.actions.iter().filter(|a| f(&a.kind)).count()
    }
}

fn drain(c: &mut Controls, now: u64, sink: &mut Sink) {
    for a in c.poll(now, MAX_ACTIONS) {
        if let Some(frame) = a.encode() {
            sink.bytes.extend_from_slice(&frame);
        } else if matches!(a.kind, ActionKind::Command(_)) {
            panic!("a controller_command action encodes: {a:?}");
        }
        sink.actions.push(a);
    }
}

/// Run the clock from `from` to `to` in 1 ms polls, draining actions.
fn run(c: &mut Controls, from: u64, to: u64, sink: &mut Sink) {
    let mut t = from;
    while t <= to {
        drain(c, t, sink);
        t += MS;
    }
}

/// A clean press of `ms` milliseconds at `at`, then 100 ms of quiet. Returns
/// the time after.
fn press(c: &mut Controls, input: Input, at: u64, ms: u64, sink: &mut Sink) -> u64 {
    c.level(input, true, at).expect("the class has the input");
    run(c, at, at + ms * MS - MS, sink);
    c.level(input, false, at + ms * MS).unwrap();
    run(c, at + ms * MS, at + ms * MS + 100 * MS, sink);
    at + ms * MS + 101 * MS
}

fn command(a: &Action) -> Option<&v2::ControllerCommand> {
    match &a.kind {
        ActionKind::Command(c) => Some(c),
        _ => None,
    }
}

fn compare_fixture(sink: &Sink, name: &str) {
    let expected = parse_hex(&fixture(name));
    assert!(!expected.is_empty(), "{name} is readable");
    assert_eq!(
        sink.bytes,
        expected,
        "the Rust controls produced exactly {name} ({} bytes produced, {} committed)",
        sink.bytes.len(),
        expected.len()
    );
    // Every produced frame decodes back as a controller_command.
    let mut at = 0;
    let mut frames = 0;
    while at < sink.bytes.len() {
        let d = v2::decode_frame(&sink.bytes[at..]);
        assert!(
            matches!(d.outcome, Outcome::Decoded(Message::ControllerCommand(_))) && d.consumed > 0,
            "frame {frames} decodes as controller_command"
        );
        at += d.consumed;
        frames += 1;
    }
    println!(
        "{name}: {} bytes, {frames} frames, byte for byte equal",
        sink.bytes.len()
    );
}

#[test]
fn the_command_values_are_the_catalogs() {
    for (value, name) in [
        (3u8, "toggle"),
        (4, "next"),
        (5, "previous"),
        (7, "volume_step"),
        (9, "join"),
        (10, "leave"),
    ] {
        assert_eq!(v2::Command::from_wire(value).map(|c| c.name()), Some(name));
    }
}

#[test]
fn the_profiles_are_the_owners_decisions() {
    let buttons = [
        Input::PlayPause,
        Input::VolumeUp,
        Input::VolumeDown,
        Input::Next,
        Input::Previous,
    ];
    let has_only = |class: SpeakerClass, want: &[Input]| {
        let p = class.profile();
        Input::ALL.iter().all(|i| p.has(*i) == want.contains(i))
    };
    let mut compact = buttons.to_vec();
    compact.push(Input::MicMuteSwitch);
    assert!(has_only(SpeakerClass::Compact, &compact));
    assert!(SpeakerClass::Compact.profile().has_microphone);
    assert_eq!(SpeakerClass::Compact.profile().led, LedKind::Status);
    assert!(has_only(SpeakerClass::TwoWay, &[Input::Pairing]));
    assert_eq!(SpeakerClass::TwoWay.profile().led, LedKind::RearStatus);
    assert!(!SpeakerClass::TwoWay.profile().led_follows_visualizer);
    assert!(has_only(
        SpeakerClass::Subwoofer,
        &[Input::Pairing, Input::SubLevelKnob, Input::SubPhaseKnob]
    ));
    let mut amp = buttons.to_vec();
    amp.push(Input::Pairing);
    assert!(has_only(SpeakerClass::StreamingAmp, &amp));
    assert!(!SpeakerClass::StreamingAmp.profile().has_microphone);
    assert_eq!(
        SpeakerClass::from_name("two-way"),
        Some(SpeakerClass::TwoWay)
    );
    assert_eq!(SpeakerClass::from_name("soundbar"), None);
}

#[test]
fn the_compact_class() {
    let mut c = Controls::new(SpeakerClass::Compact, "kitchen", "downstairs").unwrap();
    let mut sink = Sink::default();
    let mut t = 1000 * MS;

    // Contact chatter: five edges 3 ms apart, then held down 80 ms.
    for i in 0..5u64 {
        c.level(Input::PlayPause, i % 2 == 0, t + i * 3 * MS)
            .unwrap();
        drain(&mut c, t + i * 3 * MS, &mut sink);
    }
    c.level(Input::PlayPause, true, t + 15 * MS).unwrap();
    run(&mut c, t + 15 * MS, t + 95 * MS, &mut sink);
    c.level(Input::PlayPause, false, t + 95 * MS).unwrap();
    run(&mut c, t + 95 * MS, t + 200 * MS, &mut sink);
    assert_eq!(sink.actions.len(), 1, "a bouncing press is ONE toggle");
    assert_eq!(
        command(&sink.actions[0]).map(|c| c.command),
        Some(v2::Command::Toggle)
    );
    assert_eq!(
        sink.actions[0].at_ns,
        t + 115 * MS,
        "dated when the release settled"
    );
    t += 300 * MS;

    // A press shorter than the debounce is noise.
    c.level(Input::Next, true, t).unwrap();
    run(&mut c, t, t + 10 * MS, &mut sink);
    c.level(Input::Next, false, t + 11 * MS).unwrap();
    run(&mut c, t + 11 * MS, t + 100 * MS, &mut sink);
    assert_eq!(sink.actions.len(), 1, "an 11 ms blip on next sends nothing");
    t += 200 * MS;

    t = press(&mut c, Input::VolumeUp, t, 100, &mut sink);
    let before = sink.actions.len();
    t = press(&mut c, Input::VolumeDown, t, 1000, &mut sink);
    assert_eq!(
        sink.actions.len() - before,
        3,
        "volume down held 1.0 s: one step and two repeats"
    );
    t = press(&mut c, Input::Next, t, 100, &mut sink);
    t = press(&mut c, Input::Previous, t, 100, &mut sink);

    // Long press while alone: join the configured target.
    let before = sink.actions.len();
    t = press(&mut c, Input::PlayPause, t, 1500, &mut sink);
    assert_eq!(sink.actions.len() - before, 1);
    let join = command(&sink.actions[before]).unwrap();
    assert_eq!(join.command, v2::Command::Join);
    assert_eq!(join.target, "downstairs");

    // The server says the room now plays in "downstairs"; long press leaves.
    c.state(&ControllerState {
        volume: 40,
        muted: false,
        playback: Playback::Playing,
        group: "downstairs".to_string(),
    });
    t = press(&mut c, Input::PlayPause, t, 1500, &mut sink);
    assert_eq!(
        command(sink.actions.last().unwrap()).map(|c| c.command),
        Some(v2::Command::Leave)
    );

    assert!(c.level(Input::Pairing, true, t).is_err());
    assert!(c.knob(Input::SubLevelKnob, 100, t).is_err());
    assert_eq!(c.refused_inputs(), 2);
    compare_fixture(&sink, "compact.hex");
}

#[test]
fn the_mute_switch_cuts_the_microphone() {
    let mut c = Controls::new(SpeakerClass::Compact, "kitchen", "").unwrap();
    let mut sink = Sink::default();
    let input = [100i16, -200, 300, -400];
    let mut out = [0i16; 4];
    assert_eq!(
        c.mic_pass(&input, &mut out),
        0,
        "closed before the switch is read"
    );
    c.level(Input::MicMuteSwitch, false, 0).unwrap();
    run(&mut c, 0, 30 * MS, &mut sink);
    assert_eq!(c.mic_pass(&input, &mut out), 4);
    assert_eq!(out[3], -400);
    c.level(Input::MicMuteSwitch, true, 40 * MS).unwrap();
    run(&mut c, 40 * MS, 50 * MS, &mut sink);
    assert!(
        c.mic_live(),
        "a muting edge not yet debounced has not closed the gate"
    );
    run(&mut c, 51 * MS, 70 * MS, &mut sink);
    let mut out = [0i16; 4];
    assert!(!c.mic_live());
    assert_eq!(c.mic_pass(&input, &mut out), 0);
    assert_eq!(out[0], 0);
    assert_eq!(sink.count(|k| matches!(k, ActionKind::MicMute(_))), 2);
    assert_eq!(sink.actions.last().unwrap().kind, ActionKind::MicMute(true));
    assert!(sink.bytes.is_empty(), "the mute switch sends nothing");

    let mut sub = Controls::new(SpeakerClass::Subwoofer, "den", "").unwrap();
    assert_eq!(sub.mic_pass(&input, &mut out), 0);
    assert!(sub.level(Input::MicMuteSwitch, false, 0).is_err());

    let muted = LedInputs {
        booted: true,
        link_up: true,
        adopted: true,
        playing: true,
        mic_muted: true,
        pairing: false,
        fault: false,
    };
    assert_eq!(muted.decide(), LedState::Muted);
}

#[test]
fn the_two_way_class() {
    let mut c = Controls::new(SpeakerClass::TwoWay, "living", "").unwrap();
    let mut sink = Sink::default();
    let t = press(&mut c, Input::Pairing, 0, 100, &mut sink);
    assert_eq!(sink.count(|k| *k == ActionKind::Pairing), 1);
    assert!(sink.bytes.is_empty());
    assert!(c.level(Input::PlayPause, true, t).is_err());
    assert!(c.level(Input::VolumeUp, true, t).is_err());
}

#[test]
fn the_subwoofer_class() {
    let mut c = Controls::new(SpeakerClass::Subwoofer, "den", "").unwrap();
    let mut sink = Sink::default();
    press(&mut c, Input::Pairing, 0, 100, &mut sink);
    assert_eq!(sink.count(|k| *k == ActionKind::Pairing), 1);

    c.knob(Input::SubLevelKnob, 0, 0).unwrap();
    drain(&mut c, 0, &mut sink);
    assert_eq!(c.sub_level_tenths_db(), -120);
    c.knob(Input::SubLevelKnob, KNOB_MAX, MS).unwrap();
    drain(&mut c, MS, &mut sink);
    assert_eq!(c.sub_level_tenths_db(), 0, "a cut, never a boost");
    let mut max_seen = i32::MIN;
    for code in (0..=KNOB_MAX).step_by(7) {
        c.knob(Input::SubLevelKnob, code, 2 * MS).unwrap();
        max_seen = max_seen.max(c.sub_level_tenths_db());
    }
    drain(&mut c, 2 * MS, &mut sink);
    assert!(max_seen <= 0);

    c.knob(Input::SubPhaseKnob, 2000, 3 * MS).unwrap();
    drain(&mut c, 3 * MS, &mut sink);
    let settled = c.sub_phase_deg();
    let before = sink.count(|k| matches!(k, ActionKind::SubPhase(_)));
    let edge = (((settled / 15 + 1) as u64 * 4096) / 13) as u32;
    for i in 0..20u64 {
        let code = edge + if i % 2 == 1 { 10 } else { 0 } - 5;
        c.knob(Input::SubPhaseKnob, code, (4 + i) * MS).unwrap();
    }
    drain(&mut c, 30 * MS, &mut sink);
    assert_eq!(sink.count(|k| matches!(k, ActionKind::SubPhase(_))), before);
    assert_eq!(
        c.sub_phase_deg(),
        settled,
        "noise at a step edge does not chatter"
    );
    c.knob(Input::SubPhaseKnob, KNOB_MAX, 40 * MS).unwrap();
    drain(&mut c, 40 * MS, &mut sink);
    assert_eq!(c.sub_phase_deg(), 180);
    assert!(c.knob(Input::SubPhaseKnob, 4096, 41 * MS).is_err());
    assert!(sink.bytes.is_empty(), "the knobs send nothing");
}

#[test]
fn the_streaming_amp_class() {
    let mut c = Controls::new(SpeakerClass::StreamingAmp, "rack", "").unwrap();
    let mut sink = Sink::default();
    let mut t = 0;
    t = press(&mut c, Input::PlayPause, t, 100, &mut sink);
    t = press(&mut c, Input::VolumeUp, t, 100, &mut sink);
    t = press(&mut c, Input::Next, t, 100, &mut sink);
    t = press(&mut c, Input::Previous, t, 100, &mut sink);
    t = press(&mut c, Input::PlayPause, t, 1500, &mut sink);
    assert_eq!(
        sink.count(|k| *k == ActionKind::NoJoinTarget),
        1,
        "a long press with no join target sends nothing and says why"
    );
    press(&mut c, Input::Pairing, t, 100, &mut sink);
    assert_eq!(sink.count(|k| *k == ActionKind::Pairing), 1);
    compare_fixture(&sink, "streaming-amp.hex");
}

fn offer_sequence(led: &mut Led) -> usize {
    let bytes = parse_hex(&fixture("visualizer-sequence.hex"));
    let mut at = 0;
    let mut frames = 0;
    while at < bytes.len() {
        let d = v2::decode_frame(&bytes[at..]);
        let Outcome::Decoded(m) = d.outcome else {
            panic!("every frame of the sequence decodes");
        };
        assert!(led.offer(&m), "the LED takes the frame at {at}");
        at += d.consumed;
        frames += 1;
    }
    frames
}

#[test]
fn the_led_follows_the_visualizer_fixture() {
    let mut led = Led::new(&SpeakerClass::Compact.profile());
    assert_eq!(offer_sequence(&mut led), 7);
    let mut moments = 0;
    for line in fixture("visualizer-sequence.led").lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let w: Vec<&str> = line.split_whitespace().collect();
        assert_eq!(w.len(), 6, "a .led line parses: {line}");
        let at: u64 = w[0].parse().unwrap();
        let state = LedState::from_name(w[1]).expect("a state name");
        let want = LedOutput {
            red: w[2].parse().unwrap(),
            green: w[3].parse().unwrap(),
            blue: w[4].parse().unwrap(),
            brightness: w[5].parse().unwrap(),
        };
        let got = led.render(state, at);
        assert_eq!(got, want, "at {at} ns ({})", w[1]);
        moments += 1;
    }
    assert_eq!(moments, 10);
    println!("visualizer-sequence.led: {moments} moments, every one equal");

    // The two-way's rear status light never follows the visualizer (K68).
    let mut rear = Led::new(&SpeakerClass::TwoWay.profile());
    offer_sequence(&mut rear);
    let steady = rear.render(LedState::Playing, 999_999_999);
    for at in (1_000_000_000u64..=1_080_000_000).step_by(10_000_000) {
        assert_eq!(rear.render(LedState::Playing, at), steady);
    }

    let b = |v: [u8; 7]| LedInputs {
        booted: v[0] == 1,
        link_up: v[1] == 1,
        adopted: v[2] == 1,
        playing: v[3] == 1,
        mic_muted: v[4] == 1,
        pairing: v[5] == 1,
        fault: v[6] == 1,
    };
    let order = [
        ([1, 1, 1, 1, 1, 1, 1], LedState::Fault),
        ([1, 1, 1, 1, 1, 1, 0], LedState::Pairing),
        ([1, 1, 1, 1, 1, 0, 0], LedState::Muted),
        ([0, 0, 0, 0, 0, 0, 0], LedState::Boot),
        ([1, 0, 1, 1, 0, 0, 0], LedState::LinkDown),
        ([1, 1, 1, 1, 0, 0, 0], LedState::Playing),
        ([1, 1, 0, 1, 0, 0, 0], LedState::Idle),
    ];
    for (inputs, want) in order {
        assert_eq!(b(inputs).decide(), want);
    }
}
