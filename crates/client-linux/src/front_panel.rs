//! The front panel: a Linux endpoint's buttons and status LED as the
//! controller role (K65, K70, K74, K96; `docs/decisions/0080-*`).
//!
//! # One model, two bindings
//!
//! What a press means (debounce, long press, repeat, join and leave) and what
//! the LED shows are decided by `chorus-controls`, the Rust twin of the
//! firmware's `controls.c`, held to the same committed fixtures. This module
//! is only the Linux binding: it reads key events, stamps them, hands them to
//! the model, sends the model's `controller_command` frames on the session,
//! and writes the model's LED output. Nothing about a button bypasses the
//! server: a command is a request the server translates and clamps (K81,
//! I10).
//!
//! # Buttons: the kernel's gpio-keys driver, read through evdev
//!
//! A device-tree overlay (on a Raspberry Pi, `dtoverlay=gpio-key,...` per
//! button; README read 2026-09-30 at
//! <https://github.com/raspberrypi/firmware/blob/master/boot/overlays/README>)
//! turns each GPIO line into a key of an input device, `/dev/input/eventN`.
//! Reading it needs no ioctl: "you'll always get a whole number of input
//! events on a read", each a `struct input_event { struct timeval time;
//! unsigned short type; unsigned short code; int value; }`, and for `EV_KEY`
//! "0 for EV_KEY for release, 1 for keypress and 2 for autorepeat"
//! (<https://docs.kernel.org/input/input.html>, read 2026-09-30). On the two
//! targets chorus packages (arm64, x86_64, both LP64) `timeval` is two 64-bit
//! words, so a record is 24 bytes in native byte order (the layout as the
//! `libc` crate 0.2.189 declares it, MIT OR Apache-2.0,
//! <https://docs.rs/libc/0.2.189/src/libc/unix/linux_like/linux/mod.rs.html>,
//! read 2026-09-30). `EV_SYN` is 0, `EV_KEY` is 1 (the `evdev` crate 0.13.2,
//! Apache-2.0 OR MIT, `EventType::SYNCHRONIZATION = 0x00`, `KEY = 0x01`,
//! <https://docs.rs/evdev/0.13.2/src/evdev/constants.rs.html>, read
//! 2026-09-30). The kernel source and headers were not opened.
//!
//! Every event is stamped with the MONOTONIC timeline at the moment the read
//! returned (BRIEF.md guardrail 4), never with the event's own `time`, which
//! is a `timeval` the kernel fills from a clock this process does not choose.
//! gpio-keys debounces in the kernel too ("debounce-interval ... If not
//! specified defaults to 5" ms,
//! <https://www.kernel.org/doc/Documentation/devicetree/bindings/input/gpio-keys.yaml>,
//! read 2026-09-30); the model's own 20 ms debounce (ASSUMED) runs on top, so
//! a Linux panel and an ESP32 panel behave alike.
//!
//! # The LED: the kernel's LED class
//!
//! `/sys/class/leds/<name>/brightness`, 0 to `max_brightness`
//! (<https://docs.kernel.org/leds/leds-class.html>, read 2026-09-30); an RGB
//! LED through the multicolour class adds `multi_index` (the colour order) and
//! `multi_intensity`, and the kernel drives each colour at "brightness *
//! multi_intensity/max_brightness"
//! (<https://docs.kernel.org/leds/leds-class-multicolor.html>, read
//! 2026-09-30). A single-colour LED (`dtoverlay=gpio-led`, whose
//! `max_brightness` is 1) shows the model's brightness only.
//!
//! # Fakes
//!
//! Tests hand [`FrontPanel::start`] a pipe of `input_event` records and point
//! [`LedWriter::open`] at a temporary directory shaped like
//! `/sys/class/leds/<name>`. The shipped binary has no fake: it reads the
//! paths its panel configuration names, and nothing else.

use std::fmt;
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use chorus_controls::model::MAX_ACTIONS;
use chorus_controls::{ActionKind, Controls, Input, Led, LedInputs, LedOutput, SpeakerClass};
use chorus_protocol::v2::{roles, Message};

/// Bytes in one `struct input_event` on an LP64 target (arm64, x86_64).
pub const EVENT_LEN: usize = 24;
/// `EV_SYN`.
pub const EV_SYN: u16 = 0;
/// `EV_KEY`.
pub const EV_KEY: u16 = 1;

/// ASSUMED: how often the model is polled while no event arrives. The model
/// dates a settled level at its settle time whatever the poll, so this sets
/// only how late a long press or a repeat can go out, not when it is dated.
pub const POLL_INTERVAL: Duration = Duration::from_millis(5);

/// One decoded `input_event`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InputEvent {
    /// `type`.
    pub kind: u16,
    /// `code`.
    pub code: u16,
    /// `value`.
    pub value: i32,
}

impl InputEvent {
    /// Decode one record; its `time` is read past and never used.
    pub fn decode(record: &[u8; EVENT_LEN]) -> InputEvent {
        InputEvent {
            kind: u16::from_ne_bytes([record[16], record[17]]),
            code: u16::from_ne_bytes([record[18], record[19]]),
            value: i32::from_ne_bytes([record[20], record[21], record[22], record[23]]),
        }
    }

    /// Encode a record with a zero `time`, as a test's fake device writes it.
    pub fn encode(&self) -> [u8; EVENT_LEN] {
        let mut r = [0u8; EVENT_LEN];
        r[16..18].copy_from_slice(&self.kind.to_ne_bytes());
        r[18..20].copy_from_slice(&self.code.to_ne_bytes());
        r[20..24].copy_from_slice(&self.value.to_ne_bytes());
        r
    }
}

/// A front panel's configuration: `key=value`-free lines of words, `#`
/// comments. See `config/front-panel/rack-amp.conf` for the ASSUMED example.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PanelConfig {
    /// The speaker class whose controls the panel has.
    pub speaker_class: SpeakerClass,
    /// The room the panel's endpoint plays (leave returns it there), or
    /// `None` for the client's `--zone`.
    pub room: Option<String>,
    /// The group a long press of play/pause joins, or "" for none.
    pub join_target: String,
    /// The evdev device the buttons arrive on.
    pub input: PathBuf,
    /// Key code to control.
    pub keys: Vec<(u16, Input)>,
    /// The LED class directory, if the panel has a light.
    pub led: Option<PathBuf>,
}

/// Why a panel configuration was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PanelRefused(pub String);

impl fmt::Display for PanelRefused {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "front panel refused: {}", self.0)
    }
}

impl std::error::Error for PanelRefused {}

impl PanelConfig {
    /// Parse the text of a panel configuration.
    pub fn parse(text: &str) -> Result<PanelConfig, PanelRefused> {
        let mut speaker_class = None;
        let mut room = None;
        let mut join_target = String::new();
        let mut input = None;
        let mut keys: Vec<(u16, Input)> = Vec::new();
        let mut led = None;
        for (n, raw) in text.lines().enumerate() {
            let line = raw.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            let bad = |why: &str| PanelRefused(format!("line {}: {} ({})", n + 1, why, line));
            let words: Vec<&str> = line.split_whitespace().collect();
            match words.as_slice() {
                ["class", name] => {
                    speaker_class = Some(SpeakerClass::from_name(name).ok_or_else(|| {
                        bad("not a speaker class (compact, two-way, subwoofer, streaming-amp)")
                    })?)
                }
                ["room", name] => room = Some(name.to_string()),
                ["join-target", name] => join_target = name.to_string(),
                ["input", path] => input = Some(PathBuf::from(path)),
                ["led", path] => led = Some(PathBuf::from(path)),
                ["key", code, control] => {
                    let code: u16 = code.parse().map_err(|_| bad("a key code is 0 to 65535"))?;
                    let control = Input::from_name(control).ok_or_else(|| bad("not a control"))?;
                    if keys.iter().any(|(c, _)| *c == code) {
                        return Err(bad("key code mapped twice"));
                    }
                    keys.push((code, control));
                }
                _ => return Err(bad("not a panel line")),
            }
        }
        let speaker_class =
            speaker_class.ok_or_else(|| PanelRefused("no `class` line".to_string()))?;
        let input = input.ok_or_else(|| PanelRefused("no `input` line".to_string()))?;
        let profile = speaker_class.profile();
        for (code, control) in &keys {
            if !profile.has(*control) {
                return Err(PanelRefused(format!(
                    "key {} is mapped to {}, which the {} class does not have",
                    code,
                    control.name(),
                    speaker_class.name()
                )));
            }
            if matches!(control, Input::SubLevelKnob | Input::SubPhaseKnob) {
                return Err(PanelRefused(format!(
                    "key {} is mapped to a knob; knobs are ADC readings, not keys",
                    code
                )));
            }
        }
        Ok(PanelConfig {
            speaker_class,
            room,
            join_target,
            input,
            keys,
            led,
        })
    }

    /// Read and parse the file at `path`.
    pub fn load(path: &Path) -> Result<PanelConfig, PanelRefused> {
        let text = fs::read_to_string(path)
            .map_err(|e| PanelRefused(format!("{}: {}", path.display(), e)))?;
        PanelConfig::parse(&text).map_err(|e| PanelRefused(format!("{}: {}", path.display(), e.0)))
    }

    /// The roles this panel adds to the endpoint's `hello`: controller, and
    /// visualizer when it has a light that follows it.
    pub fn roles(&self) -> u16 {
        let follows = self.speaker_class.profile().led_follows_visualizer;
        roles::CONTROLLER
            | if self.led.is_some() && follows {
                roles::VISUALIZER
            } else {
                0
            }
    }

    fn control_for(&self, code: u16) -> Option<Input> {
        self.keys.iter().find(|(c, _)| *c == code).map(|(_, i)| *i)
    }
}

/// A light in the kernel's LED class.
#[derive(Debug)]
pub struct LedWriter {
    dir: PathBuf,
    max_brightness: u32,
    /// For a multicolour LED, the colour of each `multi_intensity` slot.
    colours: Option<Vec<String>>,
    last: Option<LedOutput>,
}

impl LedWriter {
    /// Open the LED at `dir` (`/sys/class/leds/<name>`): read its
    /// `max_brightness`, and its `multi_index` if it is multicolour.
    pub fn open(dir: &Path) -> io::Result<LedWriter> {
        let read = |name: &str| fs::read_to_string(dir.join(name));
        let max_brightness: u32 = read("max_brightness")?.trim().parse().map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("{}/max_brightness is not a number", dir.display()),
            )
        })?;
        let colours = match read("multi_index") {
            Ok(text) => Some(text.split_whitespace().map(str::to_string).collect()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => None,
            Err(e) => return Err(e),
        };
        Ok(LedWriter {
            dir: dir.to_path_buf(),
            max_brightness,
            colours,
            last: None,
        })
    }

    /// Show `out`, writing only when it changed.
    pub fn show(&mut self, out: LedOutput) -> io::Result<()> {
        if self.last == Some(out) {
            return Ok(());
        }
        let scaled = if out.brightness == 0 {
            0
        } else {
            ((u32::from(out.brightness) * self.max_brightness + 127) / 255).max(1)
        };
        if let Some(colours) = &self.colours {
            let intensity: Vec<String> = colours
                .iter()
                .map(|c| match c.as_str() {
                    "red" => out.red,
                    "green" => out.green,
                    "blue" => out.blue,
                    _ => 0,
                })
                .map(|v| v.to_string())
                .collect();
            fs::write(self.dir.join("multi_intensity"), intensity.join(" "))?;
        }
        fs::write(self.dir.join("brightness"), scaled.to_string())?;
        self.last = Some(out);
        Ok(())
    }
}

/// Where a command goes: the session's writer while a session is up.
pub type Uplink = Box<dyn FnMut(&Message) -> io::Result<()> + Send>;

enum PanelEvent {
    Key { code: u16, level: bool, at_ns: u64 },
    Server(Message),
    ReaderEnded(String),
}

/// What the panel has done, for the status lines and the tests.
#[derive(Debug, Default)]
pub struct PanelCounters {
    /// Key events read.
    pub keys_read: AtomicU64,
    /// Key events whose code the configuration does not map.
    pub keys_unmapped: AtomicU64,
    /// `controller_command` frames sent.
    pub commands_sent: AtomicU64,
    /// Commands decided while no session was up, or whose send failed.
    pub commands_unsent: AtomicU64,
    /// `controller_state` messages applied.
    pub states_applied: AtomicU64,
    /// LED writes that failed.
    pub led_failures: AtomicU64,
}

/// A running front panel.
pub struct FrontPanel {
    events: Sender<PanelEvent>,
    uplink: Arc<Mutex<Option<Uplink>>>,
    link_up: Arc<AtomicBool>,
    playing: Arc<AtomicBool>,
    keep: Arc<AtomicBool>,
    counters: Arc<PanelCounters>,
    threads: Vec<JoinHandle<()>>,
}

impl fmt::Debug for FrontPanel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FrontPanel").finish_non_exhaustive()
    }
}

impl FrontPanel {
    /// Start the panel: one thread reading `keys` (the evdev device, or a
    /// test's pipe), one running the model.
    ///
    /// `now_ns` is the monotonic timeline every event is stamped with at read
    /// time; `server_now_ns` is the server timeline the LED shows visualizer
    /// frames against; `log` takes one status line per event worth saying.
    pub fn start<R>(
        config: &PanelConfig,
        room: &str,
        mut keys: R,
        mut led: Option<LedWriter>,
        now_ns: Arc<dyn Fn() -> u64 + Send + Sync>,
        server_now_ns: Arc<dyn Fn() -> u64 + Send + Sync>,
        log: Arc<dyn Fn(&str) + Send + Sync>,
    ) -> Result<FrontPanel, PanelRefused>
    where
        R: Read + Send + 'static,
    {
        let room = config.room.clone().unwrap_or_else(|| room.to_string());
        let mut controls = Controls::new(config.speaker_class, &room, &config.join_target)
            .ok_or_else(|| PanelRefused("the room or join target is too long".to_string()))?;
        let mut light = Led::new(controls.profile());
        let (events, inbox) = mpsc::channel::<PanelEvent>();
        let uplink: Arc<Mutex<Option<Uplink>>> = Arc::new(Mutex::new(None));
        let link_up = Arc::new(AtomicBool::new(false));
        let playing = Arc::new(AtomicBool::new(false));
        let keep = Arc::new(AtomicBool::new(true));
        let counters = Arc::new(PanelCounters::default());
        let mut threads = Vec::new();

        // The reader: blocks on the device, stamps each whole record with the
        // monotonic timeline when the read returned, and hands key edges on.
        {
            let events = events.clone();
            let now_ns = Arc::clone(&now_ns);
            let counters = Arc::clone(&counters);
            threads.push(thread::spawn(move || {
                let mut pending: Vec<u8> = Vec::with_capacity(EVENT_LEN * 64);
                let mut buf = [0u8; EVENT_LEN * 64];
                loop {
                    let n = match keys.read(&mut buf) {
                        Ok(0) => {
                            let _ = events.send(PanelEvent::ReaderEnded(
                                "the input device closed".to_string(),
                            ));
                            return;
                        }
                        Ok(n) => n,
                        Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                        Err(e) => {
                            let _ = events.send(PanelEvent::ReaderEnded(e.to_string()));
                            return;
                        }
                    };
                    let at_ns = now_ns();
                    pending.extend_from_slice(&buf[..n]);
                    let whole = pending.len() / EVENT_LEN * EVENT_LEN;
                    for record in pending[..whole].chunks_exact(EVENT_LEN) {
                        let mut r = [0u8; EVENT_LEN];
                        r.copy_from_slice(record);
                        let e = InputEvent::decode(&r);
                        // Autorepeat (2) is the kernel's; the model repeats
                        // by its own rule, so only press and release count.
                        if e.kind != EV_KEY || !(e.value == 0 || e.value == 1) {
                            continue;
                        }
                        counters.keys_read.fetch_add(1, Ordering::Relaxed);
                        let edge = PanelEvent::Key {
                            code: e.code,
                            level: e.value == 1,
                            at_ns,
                        };
                        if events.send(edge).is_err() {
                            return;
                        }
                    }
                    pending.drain(..whole);
                }
            }));
        }

        // The model: takes key edges and server messages, polls, sends, lights.
        {
            let config = config.clone();
            let uplink = Arc::clone(&uplink);
            let link_up = Arc::clone(&link_up);
            let playing = Arc::clone(&playing);
            let keep = Arc::clone(&keep);
            let counters = Arc::clone(&counters);
            threads.push(thread::spawn(move || {
                run_model(
                    &config,
                    &mut controls,
                    &mut light,
                    &mut led,
                    &inbox,
                    &Shared {
                        uplink,
                        link_up,
                        playing,
                        keep,
                        counters,
                    },
                    now_ns.as_ref(),
                    server_now_ns.as_ref(),
                    log.as_ref(),
                );
            }));
        }

        Ok(FrontPanel {
            events,
            uplink,
            link_up,
            playing,
            keep,
            counters,
            threads,
        })
    }

    /// A session is up: commands go out through `uplink` from now on.
    pub fn connect(&self, uplink: Uplink) {
        *lock(&self.uplink) = Some(uplink);
        self.link_up.store(true, Ordering::SeqCst);
    }

    /// The session ended: commands are counted unsent until the next one.
    pub fn disconnect(&self) {
        *lock(&self.uplink) = None;
        self.link_up.store(false, Ordering::SeqCst);
        self.playing.store(false, Ordering::SeqCst);
    }

    /// Whether audio is playing, for the LED.
    pub fn set_playing(&self, playing: bool) {
        self.playing.store(playing, Ordering::SeqCst);
    }

    /// A handle the session's message handler offers server messages to
    /// (`controller_state`, `visualizer_frame`, `color`); others are ignored.
    pub fn server_messages(&self) -> Box<dyn FnMut(&Message) + Send> {
        let events = self.events.clone();
        Box::new(move |m: &Message| {
            if matches!(
                m,
                Message::ControllerState(_) | Message::VisualizerFrame(_) | Message::Color(_)
            ) {
                let _ = events.send(PanelEvent::Server(m.clone()));
            }
        })
    }

    /// What the panel has done.
    pub fn counters(&self) -> &Arc<PanelCounters> {
        &self.counters
    }

    /// Stop the model thread. The reader thread ends when its device does;
    /// it is not waited for (a blocking read on a device cannot be woken
    /// without an ioctl or a signal, and the process is ending anyway).
    pub fn stop(mut self) {
        self.keep.store(false, Ordering::SeqCst);
        if self.threads.len() == 2 {
            let model = self.threads.remove(1);
            let _ = model.join();
        }
    }
}

struct Shared {
    uplink: Arc<Mutex<Option<Uplink>>>,
    link_up: Arc<AtomicBool>,
    playing: Arc<AtomicBool>,
    keep: Arc<AtomicBool>,
    counters: Arc<PanelCounters>,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    match m.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    }
}

#[allow(clippy::too_many_arguments)]
fn run_model(
    config: &PanelConfig,
    controls: &mut Controls,
    light: &mut Led,
    led: &mut Option<LedWriter>,
    inbox: &Receiver<PanelEvent>,
    shared: &Shared,
    now_ns: &(dyn Fn() -> u64 + Send + Sync),
    server_now_ns: &(dyn Fn() -> u64 + Send + Sync),
    log: &(dyn Fn(&str) + Send + Sync),
) {
    let mut pairing = false;
    while shared.keep.load(Ordering::SeqCst) {
        match inbox.recv_timeout(POLL_INTERVAL) {
            Ok(PanelEvent::Key { code, level, at_ns }) => match config.control_for(code) {
                Some(input) => {
                    // The class was checked against the map at load, so a
                    // refusal here cannot happen; it is counted by the model
                    // if it ever does.
                    let _ = controls.level(input, level, at_ns);
                }
                None => {
                    shared.counters.keys_unmapped.fetch_add(1, Ordering::Relaxed);
                    log(&format!("front-panel key-unmapped code={}", code));
                }
            },
            Ok(PanelEvent::Server(m)) => match &m {
                Message::ControllerState(state) => {
                    controls.state(state);
                    shared.counters.states_applied.fetch_add(1, Ordering::Relaxed);
                    log(&format!(
                        "front-panel controller-state volume={} muted={} playback={} group={}",
                        state.volume,
                        u8::from(state.muted),
                        state.playback.name(),
                        state.group
                    ));
                }
                other => {
                    light.offer(other);
                }
            },
            Ok(PanelEvent::ReaderEnded(why)) => {
                log(&format!("front-panel input-ended detail=\"{}\"", why));
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
        for action in controls.poll(now_ns(), MAX_ACTIONS) {
            match &action.kind {
                ActionKind::Command(c) => {
                    let sent = match lock(&shared.uplink).as_mut() {
                        Some(up) => up(&Message::ControllerCommand(c.clone())).is_ok(),
                        None => false,
                    };
                    let counter = if sent {
                        &shared.counters.commands_sent
                    } else {
                        &shared.counters.commands_unsent
                    };
                    counter.fetch_add(1, Ordering::Relaxed);
                    log(&format!(
                        "front-panel command={} value={} target={} input={} at_ns={} sent={}",
                        c.command.name(),
                        c.value,
                        if c.target.is_empty() { "-" } else { &c.target },
                        action.input.name(),
                        action.at_ns,
                        u8::from(sent)
                    ));
                }
                ActionKind::Pairing => {
                    // A local event until adoption (goal 14) gives a press a
                    // meaning; the LED shows pairing until the next press.
                    pairing = !pairing;
                    log(&format!("front-panel pairing={}", u8::from(pairing)));
                }
                ActionKind::NoJoinTarget => log(
                    "front-panel no-join-target detail=\"a long press asks to join a group and \
                     this panel has no join-target line\"",
                ),
                other => log(&format!("front-panel event={:?}", other)),
            }
        }
        if let Some(writer) = led.as_mut() {
            let inputs = LedInputs {
                booted: true,
                link_up: shared.link_up.load(Ordering::SeqCst),
                adopted: shared.link_up.load(Ordering::SeqCst),
                playing: shared.playing.load(Ordering::SeqCst),
                mic_muted: false,
                pairing,
                fault: false,
            };
            let out = light.render(inputs.decide(), server_now_ns());
            if writer.show(out).is_err() {
                shared.counters.led_failures.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_record_round_trips_and_its_time_is_never_read() {
        let e = InputEvent {
            kind: EV_KEY,
            code: 115,
            value: 1,
        };
        let mut r = e.encode();
        r[..16].copy_from_slice(&[0xAB; 16]);
        assert_eq!(InputEvent::decode(&r), e);
    }

    #[test]
    fn a_panel_config_is_held_to_its_class() {
        let ok = PanelConfig::parse(
            "class streaming-amp\ninput /dev/input/event0\nkey 164 play-pause # KEY_PLAYPAUSE\n",
        )
        .unwrap();
        assert_eq!(ok.control_for(164), Some(Input::PlayPause));
        assert_eq!(ok.roles(), roles::CONTROLLER);
        let refused =
            PanelConfig::parse("class two-way\ninput /dev/input/event0\nkey 115 volume-up\n");
        assert!(refused.unwrap_err().0.contains("does not have"));
        assert!(PanelConfig::parse("class streaming-amp\n").is_err());
        assert!(PanelConfig::parse(
            "class streaming-amp\ninput x\nkey 1 next\nkey 1 previous\n"
        )
        .is_err());
    }
}
