//! HDMI-CEC on the hub: the Linux endpoint as the TV's Audio System
//! (goal 13; `docs/cec.md`, `crates/cec`).
//!
//! # What crosses where
//!
//! - The TV's volume keys (`<User Control Pressed>` Volume Up, Volume Down,
//!   Mute, forwarded to logical address 5 once System Audio Mode is on)
//!   become `controller_command`s on this endpoint's session: `volume_step`
//!   by [`VOLUME_STEP`] points, or `mute_set`. The hub is the controller
//!   role (K65), exactly as a front panel is (`crate::front_panel`): the
//!   server decides what the request changes and clamps it by the room's
//!   limit and quiet hours (K81, I10). Nothing about a TV key bypasses the
//!   server.
//! - The server's `controller_state` for this endpoint's room (volume 0 to
//!   100 and mute) is what `<Report Audio Status>` answers, and is pushed to
//!   the TV unprompted after a change its keys caused.
//! - The TV's power (`crates/cec/src/role.rs`, "TV on and standby") goes into
//!   a shared [`TvPower`] the source role reads once per captured chunk, so
//!   the TV input's offered signal is audio present OR (the TV is on AND
//!   `--cec-autoplay-on-power`), and a standby ends it at once
//!   (`chorus_cec::TvSignal`).
//!
//! # Threads
//!
//! One thread runs the role on the adapter: a receive that waits at most
//! [`STEP_WAIT`], then the server's latest state, then the role's timers. It
//! is stamped by the monotonic timeline the caller hands in (BRIEF.md
//! guardrail 4); the kernel's own receive timestamps are not read.

use std::fmt;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use chorus_cec::role::{Config as RoleConfig, Effect, MuteRequest, VolumeKey};
use chorus_cec::{Adapter, AdapterError, Driver, KernelAdapter, TvPower};
use chorus_protocol::v2::{Command, ControllerCommand, ControllerState, Message};

use crate::front_panel::Uplink;

/// Volume points one TV volume key press (or repeat) asks for. ASSUMED: a
/// TV repeats a held key several times a second (CTS 11.2.13-2 holds a key
/// and expects the repeats to arrive), so a smaller step than the front
/// panel's 5 (`chorus_controls::model::VOLUME_STEP`, which repeats at its
/// own slower rate) keeps a held key from sweeping the room in a second.
pub const VOLUME_STEP: i16 = 2;

/// How long one turn of the CEC thread waits for a message. ASSUMED: short
/// enough that the server's state and the role's timers are seen within a
/// small fraction of CEC's 1 s response time.
pub const STEP_WAIT: Duration = Duration::from_millis(20);

/// After an adapter call fails (anything but the adapter going away), the
/// thread waits this long before trying again, so a fault is not a spin.
/// ASSUMED.
pub const RETRY_WAIT: Duration = Duration::from_millis(500);

/// After the adapter could not be opened or address 5 not claimed (the TV
/// off with its hot-plug line low gives no physical address on a Pi, a USB
/// dongle not plugged in yet), or after it went away, the role tries again
/// this long later, so a hub started before its TV still becomes the TV's
/// Audio System. ASSUMED.
pub const REOPEN_WAIT: Duration = Duration::from_secs(10);

/// The CEC configuration (`--cec` and its flags; `crate::config`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CecConfig {
    /// The adapter, `/dev/cecN` (`--cec`).
    pub device: String,
    /// The hub's HDMI port is the TV's ARC port and ARC is wanted
    /// (`--cec-arc`). Default off, ASSUMED: P2's ARC path goes through an
    /// extractor's optical output, and a Raspberry Pi's HDMI port is a
    /// source that cannot receive ARC audio (`cec.md` section 3, LEAD).
    pub arc: bool,
    /// The TV turning on offers the TV input's signal before any audio
    /// arrives (`--cec-autoplay-on-power on|off`). Default on (K81: the TV
    /// path plays when the TV does).
    pub autoplay_on_power: bool,
    /// The name the TV shows for the hub (`--cec-osd-name`), 1 to 14
    /// printable ASCII bytes. Default `chorus`, ASSUMED.
    pub osd_name: String,
}

impl CecConfig {
    /// `device` with the defaults above.
    pub fn new(device: &str) -> CecConfig {
        CecConfig {
            device: device.to_string(),
            arc: false,
            autoplay_on_power: true,
            osd_name: chorus_cec::role::DEFAULT_OSD_NAME.to_string(),
        }
    }

    /// The role's configuration.
    pub fn role(&self) -> RoleConfig {
        RoleConfig {
            osd_name: self.osd_name.clone(),
            arc: self.arc,
            ..RoleConfig::default()
        }
    }
}

/// What the hub's CEC has done, for status lines and tests.
#[derive(Debug, Default)]
pub struct CecCounters {
    /// `controller_command`s sent.
    pub commands_sent: AtomicU64,
    /// Commands decided while no session was up, or whose send failed.
    pub commands_unsent: AtomicU64,
    /// `controller_state`s handed to the role.
    pub states_applied: AtomicU64,
}

/// A running Audio System role.
pub struct CecRole {
    states: Sender<ControllerState>,
    uplink: Arc<Mutex<Option<Uplink>>>,
    power: Arc<TvPower>,
    keep: Arc<AtomicBool>,
    counters: Arc<CecCounters>,
    thread: Option<JoinHandle<()>>,
}

impl fmt::Debug for CecRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CecRole").finish_non_exhaustive()
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    match m.lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    }
}

impl CecRole {
    /// Run the role on the configured `/dev/cecN`, opened (and reopened)
    /// on the role's own thread.
    pub fn open(
        config: &CecConfig,
        now_ms: Arc<dyn Fn() -> u64 + Send + Sync>,
        log: Arc<dyn Fn(&str) + Send + Sync>,
    ) -> CecRole {
        let device = config.device.clone();
        let opener_log = Arc::clone(&log);
        CecRole::start(
            move || {
                let adapter = KernelAdapter::open(&device)?;
                opener_log(&format!(
                    "cec adapter device={} driver={} name=\"{}\"",
                    adapter.path(),
                    adapter.driver,
                    adapter.name
                ));
                Ok(adapter)
            },
            config,
            now_ms,
            log,
        )
    }

    /// Run the role on its own thread, on the adapter `open` gives. The
    /// thread opens it, claims logical address 5 (this blocks until the
    /// kernel has polled for it) and runs the role; an adapter that cannot
    /// be opened or claimed, or that goes away, is reported by name and tried
    /// again [`REOPEN_WAIT`] later. The endpoint plays on either way: CEC is
    /// the TV's remote, never a reason not to play.
    pub fn start<A, F>(
        mut open: F,
        config: &CecConfig,
        now_ms: Arc<dyn Fn() -> u64 + Send + Sync>,
        log: Arc<dyn Fn(&str) + Send + Sync>,
    ) -> CecRole
    where
        A: Adapter + 'static,
        F: FnMut() -> Result<A, AdapterError> + Send + 'static,
    {
        let power = Arc::new(TvPower::new());
        let (states, inbox) = mpsc::channel::<ControllerState>();
        let uplink: Arc<Mutex<Option<Uplink>>> = Arc::new(Mutex::new(None));
        let keep = Arc::new(AtomicBool::new(true));
        let counters = Arc::new(CecCounters::default());
        log(&format!(
            "cec started device={} osd_name=\"{}\" arc={} autoplay_on_power={}",
            config.device,
            config.osd_name,
            u8::from(config.arc),
            u8::from(config.autoplay_on_power)
        ));
        let role = config.role();
        let thread = {
            let uplink = Arc::clone(&uplink);
            let keep = Arc::clone(&keep);
            let counters = Arc::clone(&counters);
            let power = Arc::clone(&power);
            thread::spawn(move || {
                while keep.load(Ordering::SeqCst) {
                    let started = open().and_then(|adapter| {
                        let now_ms = Arc::clone(&now_ms);
                        let log = Arc::clone(&log);
                        Driver::start(
                            adapter,
                            role.clone(),
                            Arc::clone(&power),
                            Box::new(move || now_ms()),
                            Box::new(move |l: &str| log(l)),
                        )
                    });
                    match started {
                        Ok(driver) => run(driver, &inbox, &uplink, &keep, &counters, log.as_ref()),
                        Err(e) => log(&format!(
                            "cec unusable detail=\"{}\" retry_s={}",
                            e,
                            REOPEN_WAIT.as_secs()
                        )),
                    }
                    // Whatever CEC knew of the TV is stale until it is back.
                    power.set(chorus_cec::TvPowerState::Unknown);
                    let until = REOPEN_WAIT.as_millis() / STEP_WAIT.as_millis();
                    for _ in 0..until {
                        if !keep.load(Ordering::SeqCst) {
                            return;
                        }
                        thread::sleep(STEP_WAIT);
                    }
                }
            })
        };
        CecRole {
            states,
            uplink,
            power,
            keep,
            counters,
            thread: Some(thread),
        }
    }

    /// The TV's power, for the source role (`chorus_cec::TvSignal`).
    pub fn tv_power(&self) -> Arc<TvPower> {
        Arc::clone(&self.power)
    }

    /// A session is up: commands go out through `uplink` from now on.
    pub fn connect(&self, uplink: Uplink) {
        *lock(&self.uplink) = Some(uplink);
    }

    /// The session ended: commands are counted unsent until the next one.
    pub fn disconnect(&self) {
        *lock(&self.uplink) = None;
    }

    /// A handle the session's message handler offers server messages to;
    /// it takes `controller_state` and ignores the rest.
    pub fn server_messages(&self) -> Box<dyn FnMut(&Message) + Send> {
        let states = self.states.clone();
        Box::new(move |m: &Message| {
            if let Message::ControllerState(s) = m {
                let _ = states.send(s.clone());
            }
        })
    }

    /// What the hub's CEC has done.
    pub fn counters(&self) -> &Arc<CecCounters> {
        &self.counters
    }

    /// Stop the thread (it ends within one [`STEP_WAIT`]).
    pub fn stop(mut self) {
        self.keep.store(false, Ordering::SeqCst);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Drop for CecRole {
    fn drop(&mut self) {
        self.keep.store(false, Ordering::SeqCst);
    }
}

fn run<A: Adapter>(
    mut driver: Driver<A>,
    states: &Receiver<ControllerState>,
    uplink: &Mutex<Option<Uplink>>,
    keep: &AtomicBool,
    counters: &CecCounters,
    log: &(dyn Fn(&str) + Send + Sync),
) {
    // The room's mute as last heard, for a Mute key's toggle; `None` until
    // the server's first `controller_state`.
    let mut muted: Option<bool> = None;
    let send = |command: Command, value: i16, why: &str| {
        let c = ControllerCommand {
            command,
            value,
            target: String::new(),
        };
        let sent = match lock(uplink).as_mut() {
            Some(up) => up(&Message::ControllerCommand(c)).is_ok(),
            None => false,
        };
        let counter = if sent {
            &counters.commands_sent
        } else {
            &counters.commands_unsent
        };
        counter.fetch_add(1, Ordering::Relaxed);
        log(&format!(
            "cec command={} value={} key={} sent={}",
            command.name(),
            value,
            why,
            u8::from(sent)
        ));
    };
    while keep.load(Ordering::SeqCst) {
        let mut effects = Vec::new();
        let mut failed = None;
        while let Ok(state) = states.try_recv() {
            counters.states_applied.fetch_add(1, Ordering::Relaxed);
            muted = Some(state.muted);
            match driver.room_state(state.volume, state.muted) {
                Ok(e) => effects.extend(e),
                Err(e) => failed = Some(e),
            }
        }
        if failed.is_none() {
            match driver.step(STEP_WAIT) {
                Ok(e) => effects.extend(e),
                Err(e) => failed = Some(e),
            }
        }
        for effect in effects {
            match effect {
                Effect::Volume(VolumeKey::Up) => {
                    send(Command::VolumeStep, VOLUME_STEP, "volume-up")
                }
                Effect::Volume(VolumeKey::Down) => {
                    send(Command::VolumeStep, -VOLUME_STEP, "volume-down")
                }
                Effect::Mute(request) => {
                    let to = match request {
                        // Unknown: mute, the safe side of a toggle.
                        MuteRequest::Toggle => !muted.unwrap_or(false),
                        MuteRequest::On => true,
                        MuteRequest::Off => false,
                    };
                    // Two presses before the server answers toggle twice.
                    muted = Some(to);
                    send(Command::MuteSet, i16::from(to), "mute")
                }
                Effect::SystemAudioMode(on) => {
                    log(&format!("cec system-audio-mode on={}", u8::from(on)))
                }
                Effect::Arc(up) => log(&format!("cec arc established={}", u8::from(up))),
                // The driver logged it and set the shared state.
                Effect::TvPower(_) | Effect::Send(_) => {}
            }
        }
        match failed {
            None => {}
            Some(AdapterError::Closed) => {
                log("cec stopped reason=adapter-gone");
                return;
            }
            Some(e) => {
                log(&format!("cec adapter-error detail=\"{}\"", e));
                thread::sleep(RETRY_WAIT);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chorus_cec::codec::{build, opcode, ui, PhysicalAddress, AUDIO_SYSTEM, TV};
    use chorus_cec::{FakeBus, FakeTv, TvKind, TvPowerState};
    use chorus_protocol::v2::Playback;
    use std::time::Instant;

    fn wait(what: &str, f: impl Fn() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if f() {
                return;
            }
            thread::sleep(Duration::from_millis(5));
        }
        panic!("timed out waiting for {what}");
    }

    #[test]
    fn tv_keys_become_controller_commands_and_the_rooms_state_is_reported() {
        let bus = FakeBus::new();
        let tv = FakeTv::start(&bus, TvKind::RokuLike);
        let t0 = Instant::now();
        let lines = Arc::new(Mutex::new(Vec::<String>::new()));
        let log: Arc<dyn Fn(&str) + Send + Sync> = {
            let lines = Arc::clone(&lines);
            Arc::new(move |l: &str| lines.lock().unwrap().push(l.to_string()))
        };
        let adapter = Mutex::new(Some(bus.adapter(PhysicalAddress(0x1000))));
        let cec = CecRole::start(
            move || adapter.lock().unwrap().take().ok_or(AdapterError::Closed),
            &CecConfig::new("fake"),
            Arc::new(move || t0.elapsed().as_millis() as u64),
            log,
        );
        wait("the claim", || {
            lines
                .lock()
                .unwrap()
                .iter()
                .any(|l| l.starts_with("cec claimed"))
        });
        // No session: a key is decided and counted unsent.
        tv.send(build::system_audio_mode_request(
            TV,
            AUDIO_SYSTEM,
            Some(PhysicalAddress::TV),
        ));
        tv.press(ui::VOLUME_UP);
        wait("an unsent command", || {
            cec.counters().commands_unsent.load(Ordering::Relaxed) == 1
        });
        let commands = Arc::new(Mutex::new(Vec::<Message>::new()));
        {
            let commands = Arc::clone(&commands);
            cec.connect(Box::new(move |m| {
                commands.lock().unwrap().push(m.clone());
                Ok(())
            }));
        }
        let mut offer = cec.server_messages();
        offer(&Message::ControllerState(ControllerState {
            volume: 40,
            muted: true,
            playback: Playback::Playing,
            group: "den".into(),
        }));
        wait("the state applied", || {
            cec.counters().states_applied.load(Ordering::Relaxed) == 1
        });
        tv.press(ui::VOLUME_DOWN);
        tv.press(ui::MUTE);
        wait("two commands", || commands.lock().unwrap().len() == 2);
        let got = commands.lock().unwrap().clone();
        assert_eq!(
            got[0],
            Message::ControllerCommand(ControllerCommand {
                command: Command::VolumeStep,
                value: -VOLUME_STEP,
                target: String::new()
            })
        );
        // Muted: the toggle unmutes.
        assert_eq!(
            got[1],
            Message::ControllerCommand(ControllerCommand {
                command: Command::MuteSet,
                value: 0,
                target: String::new()
            })
        );
        // The server's answer is pushed to the TV (a key just caused it).
        let n = tv.heard().len();
        offer(&Message::ControllerState(ControllerState {
            volume: 38,
            muted: false,
            playback: Playback::Playing,
            group: "den".into(),
        }));
        let r = tv
            .expect(n, Duration::from_secs(5), |m| {
                m.opcode == Some(opcode::REPORT_AUDIO_STATUS)
            })
            .expect("reported");
        assert_eq!(r.operands, vec![38]);
        // The TV's power reaches the shared state.
        let power = cec.tv_power();
        tv.power_on(TvKind::RokuLike);
        wait("the TV on", || power.get() == TvPowerState::On);
        tv.standby();
        wait("the TV in standby", || power.get() == TvPowerState::Standby);
        cec.disconnect();
        cec.stop();
        assert!(lines
            .lock()
            .unwrap()
            .iter()
            .any(|l| l.contains("cec tv-power state=standby")));
    }

    #[test]
    fn the_adapter_going_away_is_said_and_stopping_does_not_wait_for_the_reopen() {
        let bus = FakeBus::new();
        let lines = Arc::new(Mutex::new(Vec::<String>::new()));
        let log: Arc<dyn Fn(&str) + Send + Sync> = {
            let lines = Arc::clone(&lines);
            Arc::new(move |l: &str| lines.lock().unwrap().push(l.to_string()))
        };
        let opener = bus.clone();
        let cec = CecRole::start(
            move || Ok(opener.adapter(PhysicalAddress(0x1000))),
            &CecConfig::new("fake"),
            Arc::new(|| 0),
            log,
        );
        wait("the claim", || {
            lines
                .lock()
                .unwrap()
                .iter()
                .any(|l| l.starts_with("cec claimed"))
        });
        bus.close();
        wait("the stop line", || {
            lines
                .lock()
                .unwrap()
                .iter()
                .any(|l| l == "cec stopped reason=adapter-gone")
        });
        // Stopping does not wait out the reopen.
        let t = Instant::now();
        cec.stop();
        assert!(t.elapsed() < Duration::from_secs(2));
    }
}
