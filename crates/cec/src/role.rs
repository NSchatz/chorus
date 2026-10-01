//! The Audio System role (logical address 5): what the hub answers, what it
//! asks of the server, and what it learns about the TV.
//!
//! A pure state machine. It is handed each received message, the room's
//! volume and mute when the server sends them, and the time on a monotonic
//! millisecond clock the caller owns; it hands back [`Effect`]s: messages to
//! transmit, volume and mute requests for the server, and the TV's power.
//! Nothing here touches a device, a socket or a clock, so the same code runs
//! on the kernel adapter and in the tests on the fake bus.
//!
//! # What is answered (directly addressed to 5 unless said)
//!
//! | received | answer | source |
//! |---|---|---|
//! | Give System Audio Mode Status | System Audio Mode Status [on/off] | CTS 11.2.15-4, -7 |
//! | System Audio Mode Request [PA] | broadcast Set System Audio Mode [on]; unmute | CTS 11.2.15-1, -16 |
//! | System Audio Mode Request (none) | broadcast Set System Audio Mode [off] | CTS 11.2.15-5 |
//! | Give Audio Status | Report Audio Status (the room's volume, mute bit 7) | CTS 11.2.15-9 |
//! | User Control Pressed Volume Up / Down | a volume step for the room, each press and repeat | CTS 11.2.13-1..4 |
//! | User Control Pressed Mute | a mute toggle, once per press | CTS 11.2.15-8 |
//! | Give Device Power Status | Report Power Status [on] | |
//! | Give OSD Name | Set OSD Name | |
//! | Give Physical Address | broadcast Report Physical Address [PA][5] | |
//! | Get CEC Version | CEC Version [1.4] | |
//! | Give Device Vendor ID | Feature Abort (no vendor ID), or broadcast Device Vendor ID | |
//! | Request Short Audio Descriptor | LPCM 2 ch 48 kHz only, else Feature Abort [Invalid operand] | CTS 11.2.15-13, -14 |
//! | Request ARC Initiation / Termination | only with ARC configured, else Feature Abort | CTS 11.2.17-3, -4 |
//! | Abort | Feature Abort [Refused] | kernel docs [K-MODE] |
//! | anything else directly addressed | Feature Abort [Unrecognized opcode] | Android `onReceiveCommand` |
//!
//! The CTS numbers are the Android CTS host tests for an audio device
//! (`cts/hostsidetests/hdmicec/src/android/hdmicec/cts/audio/`, Apache-2.0,
//! read 2026-10-01), which name the HDMI CTS test each one ports; the
//! answers' shapes are Android's `HdmiCecLocalDeviceAudioSystem` (same
//! licence, read 2026-10-01). Broadcasts are never answered with a Feature
//! Abort, nor is anything from Unregistered (Android's
//! `maySendFeatureAbortCommand`: "Don't reply <Feature Abort> from the
//! unregistered devices or for the broadcasted messages. See CEC 12.2").
//!
//! # Deliberate differences from an AV receiver
//!
//! - System Audio Mode off does NOT mute the room (CTS 11.2.15-17 expects a
//!   mute). A chorus room is shared with every other source; a TV turning its
//!   own speakers back on must not silence the music playing in the room.
//!   The TV input's signal ends instead (standby), which stops the TV in the
//!   room through autoplay. System Audio Mode on does unmute (11.2.15-16):
//!   someone just asked the TV to play through the hub.
//! - The hub has no standby of its own (it is a server endpoint, always on):
//!   power keys are accepted and do nothing, and Give Device Power Status is
//!   always "on".
//! - A Short Audio Descriptor request is answered whatever the System Audio
//!   Mode (Android answers only with it on); the answer is the same either
//!   way and a TV that asks first then decides from it.

use crate::codec::{
    build, opcode, ui, AbortReason, AudioStatus, Message, PhysicalAddress, PowerStatus,
    AUDIO_FORMAT_LPCM, AUDIO_SYSTEM, BROADCAST, DEVICE_TYPE_AUDIO_SYSTEM, LPCM_2CH_48K_16BIT, TV,
    UNREGISTERED,
};
use crate::power::TvPowerState;
use crate::validate::{validate, Validity};

/// The required maximum response time a follower is held to: a TV's Feature
/// Abort to our directed Set System Audio Mode is waited for this long
/// before System Audio Mode is broadcast on (CTS 11.2.15-18: "within the
/// required maximum response time of 1 second"; Android's
/// `ArcInitiationActionFromAvr` waits 1000 ms for Report ARC Initiated).
pub const RESPONSE_MS: u64 = 1_000;

/// A key held without a `<User Control Released>` is let go after this long
/// (Android's `FOLLOWER_SAFETY_TIMEOUT`, 550 ms, in `HdmiCecLocalDevice`,
/// read 2026-10-01). A Mute pressed again after it is a new press.
pub const KEY_RELEASE_MS: u64 = 550;

/// A room volume change within this long of a TV volume key is reported to
/// the TV unprompted (with System Audio Mode on), so its on-screen level
/// follows. ASSUMED: long enough for the server's round trip, short enough
/// that a change made on a phone minutes later is not pushed onto the TV's
/// screen. Android reports from its volume listener on every change (LEAD in
/// `cec.md` 2.3); chorus narrows it to changes the TV caused.
pub const REPORT_AFTER_KEY_MS: u64 = 2_000;

/// How often the TV is asked for its power status, as the fallback for TVs
/// that broadcast nothing when they turn on (`cec.md` 2.3: "Polling the TV
/// with Give Device Power Status is the reliable fallback"). ASSUMED: often
/// enough that a TV turned on without any CEC traffic starts the room within
/// half a minute, rare enough to be a negligible share of the bus.
pub const POWER_POLL_MS: u64 = 30_000;

/// The OSD name used when none is configured. ASSUMED (a name, not a
/// measurement).
pub const DEFAULT_OSD_NAME: &str = "chorus";

/// What the role is configured with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// The name the TV shows: 1 to 14 printable ASCII bytes
    /// (`<Set OSD Name>`, Android's `AsciiValidator(1, 14)`).
    pub osd_name: String,
    /// A 24-bit IEEE OUI, or `None`: chorus has none, so Give Device Vendor
    /// ID is Feature Aborted.
    pub vendor_id: Option<u32>,
    /// The `<CEC Version>` answer.
    pub cec_version: u8,
    /// Whether the hub's HDMI port is the TV's ARC port and ARC is wanted
    /// (`--cec-arc`). Off: ARC messages are Feature Aborted.
    pub arc: bool,
    /// Whether the hub asks the TV to turn System Audio Mode on when it sees
    /// the TV on (CTS 11.2.15-2). Off by default: the TV asks when its own
    /// "System audio control" setting is on.
    pub initiate_system_audio: bool,
    /// [`POWER_POLL_MS`], overridable for tests.
    pub power_poll_ms: u64,
}

impl Default for Config {
    fn default() -> Config {
        Config {
            osd_name: DEFAULT_OSD_NAME.to_string(),
            vendor_id: None,
            cec_version: crate::codec::version::V1_4,
            arc: false,
            initiate_system_audio: false,
            power_poll_ms: POWER_POLL_MS,
        }
    }
}

impl Config {
    /// Refuse a configuration the bus cannot carry, by name.
    pub fn check(&self) -> Result<(), String> {
        let n = self.osd_name.len();
        if !(1..=14).contains(&n) {
            return Err(format!(
                "an OSD name is 1 to 14 bytes, not {} ('{}')",
                n, self.osd_name
            ));
        }
        if !self.osd_name.bytes().all(|b| (0x20..=0x7e).contains(&b)) {
            return Err(format!(
                "an OSD name is printable ASCII only ('{}')",
                self.osd_name
            ));
        }
        if let Some(v) = self.vendor_id {
            if v > 0x00FF_FFFF {
                return Err(format!("a vendor ID is a 24-bit OUI, not 0x{:x}", v));
            }
        }
        if self.power_poll_ms == 0 {
            return Err("the power poll interval is not 0".to_string());
        }
        Ok(())
    }
}

/// A volume key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VolumeKey {
    /// One step up.
    Up,
    /// One step down.
    Down,
}

/// A mute request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MuteRequest {
    /// The opposite of the room's mute now.
    Toggle,
    /// Mute.
    On,
    /// Unmute.
    Off,
}

/// What the role asks its caller to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    /// Transmit this message.
    Send(Message),
    /// Step the room's volume.
    Volume(VolumeKey),
    /// Change the room's mute.
    Mute(MuteRequest),
    /// The TV's power changed.
    TvPower(TvPowerState),
    /// System Audio Mode turned on or off.
    SystemAudioMode(bool),
    /// ARC was established or ended.
    Arc(bool),
}

/// The Audio System.
#[derive(Debug)]
pub struct AudioSystem {
    config: Config,
    me: u8,
    pa: PhysicalAddress,
    sam: bool,
    /// A directed Set System Audio Mode [on] sent to the TV, broadcast at
    /// this time unless the TV Feature-Aborts it first.
    sam_pending_until: Option<u64>,
    /// The TV refused System Audio Mode; not asked again this run (CTS
    /// 11.2.15-3: the device "will need a reboot here so it'll forget").
    tv_refused_sam: bool,
    may_initiate: bool,
    arc: bool,
    room: AudioStatus,
    tv_power: TvPowerState,
    last_key: Option<(u8, u64)>,
    last_tv_volume_key_ms: Option<u64>,
    next_poll_ms: u64,
}

impl AudioSystem {
    /// The role at physical address `pa`, logical address 5.
    pub fn new(config: Config, pa: PhysicalAddress) -> AudioSystem {
        AudioSystem {
            config,
            me: AUDIO_SYSTEM,
            pa,
            sam: false,
            sam_pending_until: None,
            tv_refused_sam: false,
            may_initiate: true,
            arc: false,
            room: AudioStatus {
                muted: false,
                volume: None,
            },
            tv_power: TvPowerState::Unknown,
            last_key: None,
            last_tv_volume_key_ms: None,
            next_poll_ms: 0,
        }
    }

    /// Whether System Audio Mode is on.
    pub fn system_audio_mode(&self) -> bool {
        self.sam
    }

    /// Whether ARC is established.
    pub fn arc(&self) -> bool {
        self.arc
    }

    /// What the role last heard of the TV's power.
    pub fn tv_power(&self) -> TvPowerState {
        self.tv_power
    }

    /// The room's volume and mute as last reported.
    pub fn room(&self) -> AudioStatus {
        self.room
    }

    /// The physical address in use.
    pub fn physical_address(&self) -> PhysicalAddress {
        self.pa
    }

    /// The logical address was claimed: announce the hub, ask the TV's power,
    /// and start ARC where it is configured (CTS 11.2.17-1).
    pub fn start(&mut self, now_ms: u64) -> Vec<Effect> {
        let mut out = vec![Effect::Send(build::report_physical_address(
            self.me,
            self.pa,
            DEVICE_TYPE_AUDIO_SYSTEM,
        ))];
        out.push(Effect::Send(build::give_device_power_status(self.me, TV)));
        if self.config.arc {
            out.push(Effect::Send(build::initiate_arc(self.me, TV)));
        }
        self.next_poll_ms = now_ms + self.config.power_poll_ms;
        out
    }

    /// The physical address changed (the kernel's state-change event, a
    /// hot plug): announce it again.
    pub fn set_physical_address(&mut self, pa: PhysicalAddress) -> Vec<Effect> {
        if pa == self.pa {
            return Vec::new();
        }
        self.pa = pa;
        if !pa.is_valid() || pa == PhysicalAddress::INVALID {
            return Vec::new();
        }
        vec![Effect::Send(build::report_physical_address(
            self.me,
            pa,
            DEVICE_TYPE_AUDIO_SYSTEM,
        ))]
    }

    /// The server's `controller_state` for the hub's room: volume 0 to 100
    /// and mute. Reported to the TV unprompted when System Audio Mode is on
    /// and a TV volume key caused it ([`REPORT_AFTER_KEY_MS`]).
    pub fn room_state(&mut self, volume: u8, muted: bool, now_ms: u64) -> Vec<Effect> {
        let next = AudioStatus {
            muted,
            volume: Some(volume.min(100)),
        };
        let changed = next != self.room;
        self.room = next;
        let caused_by_tv = self
            .last_tv_volume_key_ms
            .is_some_and(|at| now_ms.saturating_sub(at) <= REPORT_AFTER_KEY_MS);
        if changed && self.sam && caused_by_tv {
            vec![Effect::Send(build::report_audio_status(self.me, TV, next))]
        } else {
            Vec::new()
        }
    }

    /// Time passing: the System Audio Mode initiation's deadline, starting
    /// one, and the TV power poll.
    pub fn tick(&mut self, now_ms: u64) -> Vec<Effect> {
        let mut out = Vec::new();
        if let Some(until) = self.sam_pending_until {
            if now_ms >= until {
                // No Feature Abort within the response time: the TV took it.
                self.sam_pending_until = None;
                self.turn_sam(true, &mut out);
            }
        } else if self.config.initiate_system_audio
            && self.may_initiate
            && !self.sam
            && !self.tv_refused_sam
            && self.tv_power == TvPowerState::On
        {
            self.may_initiate = false;
            self.sam_pending_until = Some(now_ms + RESPONSE_MS);
            out.push(Effect::Send(build::set_system_audio_mode(
                self.me, TV, true,
            )));
        }
        if now_ms >= self.next_poll_ms {
            self.next_poll_ms = now_ms + self.config.power_poll_ms;
            out.push(Effect::Send(build::give_device_power_status(self.me, TV)));
        }
        out
    }

    fn turn_sam(&mut self, on: bool, out: &mut Vec<Effect>) {
        out.push(Effect::Send(build::set_system_audio_mode(
            self.me, BROADCAST, on,
        )));
        if on && self.room.muted {
            out.push(Effect::Mute(MuteRequest::Off));
        }
        if self.sam != on {
            self.sam = on;
            out.push(Effect::SystemAudioMode(on));
        }
    }

    fn set_tv_power(&mut self, state: TvPowerState, out: &mut Vec<Effect>) {
        if state == self.tv_power {
            return;
        }
        self.tv_power = state;
        out.push(Effect::TvPower(state));
        match state {
            TvPowerState::On => self.may_initiate = true,
            TvPowerState::Standby => {
                self.sam_pending_until = None;
                // CTS 11.2.15-6: System Audio Mode off is broadcast before
                // going to standby.
                if self.sam {
                    self.turn_sam(false, out);
                }
            }
            TvPowerState::Unknown => {}
        }
    }

    /// What the TV's power is, from a valid message wherever it was going
    /// (design envelope section 3; `cec.md` 2.3, "TV turned on signals").
    fn watch_power(&mut self, m: &Message, out: &mut Vec<Effect>) {
        let Some(op) = m.opcode else { return };
        let from_tv = m.initiator == TV;
        let state = match op {
            opcode::REPORT_POWER_STATUS if from_tv => match PowerStatus::from_byte(m.operands[0]) {
                Some(PowerStatus::On | PowerStatus::StandbyToOn) => TvPowerState::On,
                Some(PowerStatus::Standby | PowerStatus::OnToStandby) => TvPowerState::Standby,
                None => return,
            },
            opcode::ACTIVE_SOURCE | opcode::ROUTING_CHANGE | opcode::SET_STREAM_PATH => {
                TvPowerState::On
            }
            opcode::IMAGE_VIEW_ON | opcode::TEXT_VIEW_ON if m.destination == TV => TvPowerState::On,
            opcode::SYSTEM_AUDIO_MODE_REQUEST if from_tv && !m.operands.is_empty() => {
                TvPowerState::On
            }
            opcode::REQUEST_ARC_INITIATION if from_tv => TvPowerState::On,
            opcode::STANDBY if from_tv || m.destination == BROADCAST => TvPowerState::Standby,
            _ => return,
        };
        self.set_tv_power(state, out);
    }

    /// One received message.
    pub fn handle(&mut self, m: &Message, now_ms: u64) -> Vec<Effect> {
        let mut out = Vec::new();
        // Our own transmissions echoed back, and polls, are not messages to
        // handle (Android: `sourceAddressIsLocal`).
        if m.initiator == self.me {
            return out;
        }
        let Some(op) = m.opcode else {
            return out;
        };
        let validity = validate(m);
        if validity == Validity::Ok {
            self.watch_power(m, &mut out);
        }
        if m.destination != self.me && m.destination != BROADCAST {
            return out;
        }
        let directed = m.destination == self.me;
        let abort = |reason: AbortReason, out: &mut Vec<Effect>| {
            if directed && m.initiator != UNREGISTERED && op != opcode::FEATURE_ABORT {
                out.push(Effect::Send(build::feature_abort(
                    AUDIO_SYSTEM,
                    m.initiator,
                    op,
                    reason,
                )));
            }
        };
        match validity {
            Validity::Ok => {}
            Validity::Parameter | Validity::ParameterLong => {
                abort(AbortReason::InvalidOperand, &mut out);
                return out;
            }
            Validity::Source | Validity::Destination | Validity::ParameterShort => return out,
        }
        let to = m.initiator;
        let me = self.me;
        match op {
            opcode::GIVE_SYSTEM_AUDIO_MODE_STATUS => {
                // Android answers "on" to the TV while its own initiation is
                // pending: the TV evidently supports the mode.
                let on = self.sam || (to == TV && self.sam_pending_until.is_some());
                out.push(Effect::Send(build::system_audio_mode_status(me, to, on)));
            }
            opcode::SYSTEM_AUDIO_MODE_REQUEST => {
                self.sam_pending_until = None;
                self.turn_sam(!m.operands.is_empty(), &mut out);
            }
            opcode::GIVE_AUDIO_STATUS => {
                out.push(Effect::Send(build::report_audio_status(me, to, self.room)));
            }
            opcode::USER_CONTROL_PRESSED => self.key(m, now_ms, &mut out, abort),
            opcode::USER_CONTROL_RELEASED => self.last_key = None,
            opcode::GIVE_DEVICE_POWER_STATUS => {
                out.push(Effect::Send(build::report_power_status(
                    me,
                    to,
                    PowerStatus::On,
                )));
            }
            opcode::GIVE_OSD_NAME => {
                out.push(Effect::Send(build::set_osd_name(
                    me,
                    to,
                    &self.config.osd_name,
                )));
            }
            opcode::GIVE_PHYSICAL_ADDRESS => {
                out.push(Effect::Send(build::report_physical_address(
                    me,
                    self.pa,
                    DEVICE_TYPE_AUDIO_SYSTEM,
                )));
            }
            opcode::GET_CEC_VERSION => {
                out.push(Effect::Send(build::cec_version(
                    me,
                    to,
                    self.config.cec_version,
                )));
            }
            opcode::GIVE_DEVICE_VENDOR_ID => match self.config.vendor_id {
                Some(oui) => out.push(Effect::Send(build::device_vendor_id(me, oui))),
                // ASSUMED reason: chorus has no OUI, and "Unrecognized
                // opcode" is what a device without the feature answers.
                None => abort(AbortReason::UnrecognizedOpcode, &mut out),
            },
            opcode::REQUEST_SHORT_AUDIO_DESCRIPTOR => {
                // Each operand byte: bits 6-7 the Audio Format ID (0: the
                // CEA-861 codes), bits 0-5 the code (Android's
                // `parseAudioCodecs`). Only LPCM is ever reported (P2).
                let lpcm = m
                    .operands
                    .iter()
                    .take(4)
                    .any(|b| b >> 6 == 0 && b & 0x3f == AUDIO_FORMAT_LPCM);
                if lpcm {
                    out.push(Effect::Send(build::report_short_audio_descriptor(
                        me,
                        to,
                        &[LPCM_2CH_48K_16BIT],
                    )));
                } else {
                    abort(AbortReason::InvalidOperand, &mut out);
                }
            }
            opcode::ABORT => abort(AbortReason::Refused, &mut out),
            opcode::REQUEST_ARC_INITIATION => {
                if self.config.arc {
                    out.push(Effect::Send(build::initiate_arc(me, to)));
                } else {
                    abort(AbortReason::UnrecognizedOpcode, &mut out);
                }
            }
            opcode::REQUEST_ARC_TERMINATION => {
                if !self.config.arc {
                    abort(AbortReason::UnrecognizedOpcode, &mut out);
                } else if self.arc {
                    out.push(Effect::Send(build::terminate_arc(me, to)));
                } else {
                    abort(AbortReason::NotInCorrectMode, &mut out);
                }
            }
            opcode::REPORT_ARC_INITIATED | opcode::REPORT_ARC_TERMINATED => {
                if self.config.arc {
                    let up = op == opcode::REPORT_ARC_INITIATED;
                    if self.arc != up {
                        self.arc = up;
                        out.push(Effect::Arc(up));
                    }
                } else {
                    abort(AbortReason::UnrecognizedOpcode, &mut out);
                }
            }
            opcode::FEATURE_ABORT => {
                // The TV refused our directed Set System Audio Mode within
                // the response time: never broadcast it (CTS 11.2.15-3, -18).
                if m.operands[0] == opcode::SET_SYSTEM_AUDIO_MODE
                    && self.sam_pending_until.is_some()
                {
                    self.sam_pending_until = None;
                    self.tv_refused_sam = true;
                }
            }
            opcode::STANDBY => {
                // From the TV, `watch_power` has done it all. From anyone
                // else, the hub "goes to standby", which for an always-on
                // hub is only System Audio Mode off (CTS 11.2.15-6).
                if to != TV && self.sam {
                    self.sam_pending_until = None;
                    self.turn_sam(false, &mut out);
                }
            }
            // Replies and announcements: noted (the TV's power above) and
            // never answered.
            opcode::REPORT_POWER_STATUS
            | opcode::REPORT_PHYSICAL_ADDRESS
            | opcode::DEVICE_VENDOR_ID
            | opcode::CEC_VERSION
            | opcode::SET_OSD_NAME
            | opcode::REPORT_AUDIO_STATUS
            | opcode::SYSTEM_AUDIO_MODE_STATUS
            | opcode::REPORT_SHORT_AUDIO_DESCRIPTOR
            | opcode::ACTIVE_SOURCE
            | opcode::ROUTING_CHANGE
            | opcode::ROUTING_INFORMATION
            | opcode::SET_STREAM_PATH
            | opcode::REQUEST_ACTIVE_SOURCE
            | opcode::SET_SYSTEM_AUDIO_MODE => {}
            _ => abort(AbortReason::UnrecognizedOpcode, &mut out),
        }
        out
    }

    fn key(
        &mut self,
        m: &Message,
        now_ms: u64,
        out: &mut Vec<Effect>,
        abort: impl Fn(AbortReason, &mut Vec<Effect>),
    ) {
        let key = m.operands[0];
        // A repeat: the same key again before a release and within the
        // safety timeout (CTS 11.2.13-2, -3: press and hold, with and without
        // the release).
        let repeat = self
            .last_key
            .is_some_and(|(k, at)| k == key && now_ms.saturating_sub(at) <= KEY_RELEASE_MS);
        self.last_key = Some((key, now_ms));
        match key {
            ui::VOLUME_UP | ui::VOLUME_DOWN => {
                self.last_tv_volume_key_ms = Some(now_ms);
                out.push(Effect::Volume(if key == ui::VOLUME_UP {
                    VolumeKey::Up
                } else {
                    VolumeKey::Down
                }));
            }
            ui::MUTE => {
                self.last_tv_volume_key_ms = Some(now_ms);
                if !repeat {
                    out.push(Effect::Mute(MuteRequest::Toggle));
                }
            }
            ui::MUTE_FUNCTION | ui::RESTORE_VOLUME_FUNCTION => {
                self.last_tv_volume_key_ms = Some(now_ms);
                out.push(Effect::Mute(if key == ui::MUTE_FUNCTION {
                    MuteRequest::On
                } else {
                    MuteRequest::Off
                }));
            }
            // Accepted and nothing to do: the hub has no standby (Android
            // does not Feature Abort a power key either).
            ui::POWER
            | ui::POWER_TOGGLE_FUNCTION
            | ui::POWER_OFF_FUNCTION
            | ui::POWER_ON_FUNCTION => {}
            _ => abort(AbortReason::InvalidOperand, out),
        }
    }
}
