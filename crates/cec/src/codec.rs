//! The CEC message: a header block, an opcode and its operands, as bytes.
//!
//! # The frame
//!
//! A CEC message is at most 16 bytes: `msg[0]` is the header block, the
//! initiator's logical address in the high nibble and the destination's in
//! the low nibble; `msg[1]` is the opcode; the rest are operands. A message
//! of the header alone is a polling message (how a logical address is
//! claimed). The kernel's `struct cec_msg` documents exactly this shape
//! ("msg[0] = initiator<<4 | destination, msg[1] = opcode, rest operands",
//! <https://docs.kernel.org/userspace-api/media/cec/cec-ioc-receive.html>,
//! read 2026-10-01) and so does Android's HDMI service, whose
//! `HdmiCecMessage` keeps source, destination, opcode and params apart
//! (Apache-2.0, <https://android.googlesource.com/platform/frameworks/base/+/refs/heads/main/services/core/java/com/android/server/hdmi/>,
//! read 2026-10-01).
//!
//! # Addresses
//!
//! Logical addresses as Android's `hdmi_cec.h` lists them (Apache-2.0,
//! <https://android.googlesource.com/platform/hardware/libhardware/+/lollipop-dev/include/hardware/hdmi_cec.h>,
//! read 2026-10-01): 0 TV, 1 and 2 recorders, 3 tuner 1, 4 playback 1, **5
//! the Audio System**, 6 and 7 tuners, 8 playback 2, 9 recorder 3, 10 tuner
//! 4, 11 playback 3, 12 and 13 reserved, 14 free use, and 15 Unregistered as
//! an initiator, Broadcast as a destination.
//!
//! # Opcodes
//!
//! The values below are the ones the MIT `cec_linux` 0.2.2 crate's opcode
//! enum carries, cross-checked against Android's `Constants.java`
//! (Apache-2.0, same tree as above), both read 2026-10-01; the research file
//! of goal 13 (`cec.md` section 2.2) tabulates them. Only the opcodes the
//! Audio System role reads or writes are named; any other byte is still a
//! valid opcode and decodes, it simply has no name here.

use std::fmt;

/// Logical address of the TV.
pub const TV: u8 = 0;
/// Logical address of the Audio System: the hub's, and the only one the role
/// ever claims (an Audio System has exactly one, `hdmi_cec.h`).
pub const AUDIO_SYSTEM: u8 = 5;
/// Logical address 3, the first tuner (CTS 11.2.15-1 sends a System Audio
/// Mode Request from it).
pub const TUNER_1: u8 = 3;
/// Logical address 4, the first playback device.
pub const PLAYBACK_1: u8 = 4;
/// Logical address 15: Unregistered as an initiator.
pub const UNREGISTERED: u8 = 15;
/// Logical address 15: Broadcast as a destination.
pub const BROADCAST: u8 = 15;

/// The longest message the bus carries, header included.
pub const MAX_MESSAGE_LEN: usize = 16;

/// The opcodes chorus names. Values: see the module documentation.
pub mod opcode {
    /// `<Feature Abort>` [opcode][reason].
    pub const FEATURE_ABORT: u8 = 0x00;
    /// `<Image View On>`, to the TV.
    pub const IMAGE_VIEW_ON: u8 = 0x04;
    /// `<Text View On>`, to the TV.
    pub const TEXT_VIEW_ON: u8 = 0x0d;
    /// `<Standby>`.
    pub const STANDBY: u8 = 0x36;
    /// `<User Control Pressed>` [UI command].
    pub const USER_CONTROL_PRESSED: u8 = 0x44;
    /// `<User Control Released>`.
    pub const USER_CONTROL_RELEASED: u8 = 0x45;
    /// `<Give OSD Name>`.
    pub const GIVE_OSD_NAME: u8 = 0x46;
    /// `<Set OSD Name>` [1 to 14 ASCII bytes].
    pub const SET_OSD_NAME: u8 = 0x47;
    /// `<System Audio Mode Request>` [physical address] (none: off).
    pub const SYSTEM_AUDIO_MODE_REQUEST: u8 = 0x70;
    /// `<Give Audio Status>`.
    pub const GIVE_AUDIO_STATUS: u8 = 0x71;
    /// `<Set System Audio Mode>` [0 off, 1 on].
    pub const SET_SYSTEM_AUDIO_MODE: u8 = 0x72;
    /// `<Report Audio Status>` [mute bit 7, volume bits 0 to 6].
    pub const REPORT_AUDIO_STATUS: u8 = 0x7a;
    /// `<Give System Audio Mode Status>`.
    pub const GIVE_SYSTEM_AUDIO_MODE_STATUS: u8 = 0x7d;
    /// `<System Audio Mode Status>` [0 off, 1 on].
    pub const SYSTEM_AUDIO_MODE_STATUS: u8 = 0x7e;
    /// `<Routing Change>` [old physical address][new physical address].
    pub const ROUTING_CHANGE: u8 = 0x80;
    /// `<Routing Information>` [physical address].
    pub const ROUTING_INFORMATION: u8 = 0x81;
    /// `<Active Source>` [physical address].
    pub const ACTIVE_SOURCE: u8 = 0x82;
    /// `<Give Physical Address>`.
    pub const GIVE_PHYSICAL_ADDRESS: u8 = 0x83;
    /// `<Report Physical Address>` [physical address][primary device type].
    pub const REPORT_PHYSICAL_ADDRESS: u8 = 0x84;
    /// `<Request Active Source>`.
    pub const REQUEST_ACTIVE_SOURCE: u8 = 0x85;
    /// `<Set Stream Path>` [physical address].
    pub const SET_STREAM_PATH: u8 = 0x86;
    /// `<Device Vendor ID>` [3-byte OUI].
    pub const DEVICE_VENDOR_ID: u8 = 0x87;
    /// `<Vendor Command>`.
    pub const VENDOR_COMMAND: u8 = 0x89;
    /// `<Give Device Vendor ID>`.
    pub const GIVE_DEVICE_VENDOR_ID: u8 = 0x8c;
    /// `<Give Device Power Status>`.
    pub const GIVE_DEVICE_POWER_STATUS: u8 = 0x8f;
    /// `<Report Power Status>` [0 on, 1 standby, 2 standby to on, 3 on to standby].
    pub const REPORT_POWER_STATUS: u8 = 0x90;
    /// `<CEC Version>` [version].
    pub const CEC_VERSION: u8 = 0x9e;
    /// `<Get CEC Version>`.
    pub const GET_CEC_VERSION: u8 = 0x9f;
    /// `<Vendor Command With ID>`.
    pub const VENDOR_COMMAND_WITH_ID: u8 = 0xa0;
    /// `<Report Short Audio Descriptor>` [1 to 4 three-byte descriptors].
    pub const REPORT_SHORT_AUDIO_DESCRIPTOR: u8 = 0xa3;
    /// `<Request Short Audio Descriptor>` [1 to 4 format bytes].
    pub const REQUEST_SHORT_AUDIO_DESCRIPTOR: u8 = 0xa4;
    /// `<Give Features>` (CEC 2.0).
    pub const GIVE_FEATURES: u8 = 0xa5;
    /// `<Initiate ARC>`, Audio System to TV.
    pub const INITIATE_ARC: u8 = 0xc0;
    /// `<Report ARC Initiated>`, TV to Audio System.
    pub const REPORT_ARC_INITIATED: u8 = 0xc1;
    /// `<Report ARC Terminated>`, TV to Audio System.
    pub const REPORT_ARC_TERMINATED: u8 = 0xc2;
    /// `<Request ARC Initiation>`, TV to Audio System.
    pub const REQUEST_ARC_INITIATION: u8 = 0xc3;
    /// `<Request ARC Termination>`, TV to Audio System.
    pub const REQUEST_ARC_TERMINATION: u8 = 0xc4;
    /// `<Terminate ARC>`, Audio System to TV.
    pub const TERMINATE_ARC: u8 = 0xc5;
    /// `<Abort>`.
    pub const ABORT: u8 = 0xff;

    /// Every named opcode with its fixture spelling.
    pub const NAMES: [(u8, &str); 39] = [
        (FEATURE_ABORT, "feature_abort"),
        (IMAGE_VIEW_ON, "image_view_on"),
        (TEXT_VIEW_ON, "text_view_on"),
        (STANDBY, "standby"),
        (USER_CONTROL_PRESSED, "user_control_pressed"),
        (USER_CONTROL_RELEASED, "user_control_released"),
        (GIVE_OSD_NAME, "give_osd_name"),
        (SET_OSD_NAME, "set_osd_name"),
        (SYSTEM_AUDIO_MODE_REQUEST, "system_audio_mode_request"),
        (GIVE_AUDIO_STATUS, "give_audio_status"),
        (SET_SYSTEM_AUDIO_MODE, "set_system_audio_mode"),
        (REPORT_AUDIO_STATUS, "report_audio_status"),
        (
            GIVE_SYSTEM_AUDIO_MODE_STATUS,
            "give_system_audio_mode_status",
        ),
        (SYSTEM_AUDIO_MODE_STATUS, "system_audio_mode_status"),
        (ROUTING_CHANGE, "routing_change"),
        (ROUTING_INFORMATION, "routing_information"),
        (ACTIVE_SOURCE, "active_source"),
        (GIVE_PHYSICAL_ADDRESS, "give_physical_address"),
        (REPORT_PHYSICAL_ADDRESS, "report_physical_address"),
        (REQUEST_ACTIVE_SOURCE, "request_active_source"),
        (SET_STREAM_PATH, "set_stream_path"),
        (DEVICE_VENDOR_ID, "device_vendor_id"),
        (VENDOR_COMMAND, "vendor_command"),
        (GIVE_DEVICE_VENDOR_ID, "give_device_vendor_id"),
        (GIVE_DEVICE_POWER_STATUS, "give_device_power_status"),
        (REPORT_POWER_STATUS, "report_power_status"),
        (CEC_VERSION, "cec_version"),
        (GET_CEC_VERSION, "get_cec_version"),
        (VENDOR_COMMAND_WITH_ID, "vendor_command_with_id"),
        (
            REPORT_SHORT_AUDIO_DESCRIPTOR,
            "report_short_audio_descriptor",
        ),
        (
            REQUEST_SHORT_AUDIO_DESCRIPTOR,
            "request_short_audio_descriptor",
        ),
        (GIVE_FEATURES, "give_features"),
        (INITIATE_ARC, "initiate_arc"),
        (REPORT_ARC_INITIATED, "report_arc_initiated"),
        (REPORT_ARC_TERMINATED, "report_arc_terminated"),
        (REQUEST_ARC_INITIATION, "request_arc_initiation"),
        (REQUEST_ARC_TERMINATION, "request_arc_termination"),
        (TERMINATE_ARC, "terminate_arc"),
        (ABORT, "abort"),
    ];

    /// The fixture spelling of `op`, if chorus names it.
    pub fn name(op: u8) -> Option<&'static str> {
        NAMES.iter().find(|(v, _)| *v == op).map(|(_, n)| *n)
    }

    /// The opcode spelled `name`.
    pub fn from_name(name: &str) -> Option<u8> {
        NAMES.iter().find(|(_, n)| *n == name).map(|(v, _)| *v)
    }
}

/// `<User Control Pressed>` UI command codes the role reads (`cec.md`
/// section 2.2, from the `cec_linux` crate's enum and Android's
/// `HdmiCecKeycode`, both read 2026-10-01).
pub mod ui {
    /// Power.
    pub const POWER: u8 = 0x40;
    /// Volume Up.
    pub const VOLUME_UP: u8 = 0x41;
    /// Volume Down.
    pub const VOLUME_DOWN: u8 = 0x42;
    /// Mute (a toggle).
    pub const MUTE: u8 = 0x43;
    /// Mute Function (mute, not a toggle).
    pub const MUTE_FUNCTION: u8 = 0x65;
    /// Restore Volume Function (unmute).
    pub const RESTORE_VOLUME_FUNCTION: u8 = 0x66;
    /// Power Toggle Function.
    pub const POWER_TOGGLE_FUNCTION: u8 = 0x6b;
    /// Power Off Function.
    pub const POWER_OFF_FUNCTION: u8 = 0x6c;
    /// Power On Function.
    pub const POWER_ON_FUNCTION: u8 = 0x6d;
}

/// `<Feature Abort>` reasons (Android `Constants.ABORT_*`, read 2026-10-01).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AbortReason {
    /// 0: the opcode is not handled.
    UnrecognizedOpcode = 0,
    /// 1: handled, but not in the device's current state.
    NotInCorrectMode = 1,
    /// 2: cannot provide the source.
    CannotProvideSource = 2,
    /// 3: an operand was invalid.
    InvalidOperand = 3,
    /// 4: refused.
    Refused = 4,
    /// 5: unable to determine.
    UnableToDetermine = 5,
}

impl AbortReason {
    /// The operand byte.
    pub fn byte(self) -> u8 {
        self as u8
    }
}

/// `<CEC Version>` operand values ([K-LOG] in `cec.md`: 4 is 1.3a, 5 is
/// 1.4, 6 is 2.0).
pub mod version {
    /// CEC 1.4: what the hub reports. It claims none of 2.0's features
    /// (`<Give Features>`), so 1.4 is the honest answer.
    pub const V1_4: u8 = 5;
    /// CEC 2.0.
    pub const V2_0: u8 = 6;
}

/// Primary device type of an Audio System in `<Report Physical Address>`
/// (5; Android's `HdmiCecMessageBuilder` and `cec.md` section 2.2).
pub const DEVICE_TYPE_AUDIO_SYSTEM: u8 = 5;

/// `<Report Power Status>` operands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerStatus {
    /// 0.
    On = 0,
    /// 1.
    Standby = 1,
    /// 2: in transition from standby to on.
    StandbyToOn = 2,
    /// 3: in transition from on to standby.
    OnToStandby = 3,
}

impl PowerStatus {
    /// The operand, if it is one of the four.
    pub fn from_byte(b: u8) -> Option<PowerStatus> {
        Some(match b {
            0 => PowerStatus::On,
            1 => PowerStatus::Standby,
            2 => PowerStatus::StandbyToOn,
            3 => PowerStatus::OnToStandby,
            _ => return None,
        })
    }
}

/// An HDMI physical address `a.b.c.d`, one nibble each, `a` in the top
/// nibble ([K-PHYS] in `cec.md`: the TV is 0.0.0.0, a device on a TV input
/// is `a`.0.0.0).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PhysicalAddress(pub u16);

impl PhysicalAddress {
    /// The TV's.
    pub const TV: PhysicalAddress = PhysicalAddress(0x0000);
    /// "No address" (`CEC_PHYS_ADDR_INVALID`, 0xffff, the MIT `cec_linux`
    /// crate's `CecPhysicalAddress::INVALID`, read 2026-10-01).
    pub const INVALID: PhysicalAddress = PhysicalAddress(0xffff);

    /// From two operand bytes, most significant first.
    pub fn from_bytes(hi: u8, lo: u8) -> PhysicalAddress {
        PhysicalAddress(u16::from_be_bytes([hi, lo]))
    }

    /// As two operand bytes, most significant first.
    pub fn bytes(self) -> [u8; 2] {
        self.0.to_be_bytes()
    }

    /// Whether the address is well formed: once a nibble is 0, every nibble
    /// after it is 0 too (Android's `isValidPhysicalAddress`, read
    /// 2026-10-01). 0xffff passes this rule and is still "no address".
    pub fn is_valid(self) -> bool {
        let mut rest = self.0;
        while rest != 0 {
            let top = rest & 0xF000;
            rest <<= 4;
            if top == 0 && rest != 0 {
                return false;
            }
        }
        true
    }
}

impl fmt::Display for PhysicalAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let v = self.0;
        write!(
            f,
            "{:x}.{:x}.{:x}.{:x}",
            v >> 12,
            (v >> 8) & 0xF,
            (v >> 4) & 0xF,
            v & 0xF
        )
    }
}

/// The mute bit and volume of `<Report Audio Status>`: bit 7 mute, bits 0
/// to 6 the volume 0 to 100 (0x64), 0x7f "unknown" (Android's
/// `buildReportAudioStatus`; `cec.md` section 2.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioStatus {
    /// Muted.
    pub muted: bool,
    /// 0 to 100, or `None` when not known yet (sent as 0x7f).
    pub volume: Option<u8>,
}

impl AudioStatus {
    /// The volume operand for "unknown".
    pub const UNKNOWN_VOLUME: u8 = 0x7f;

    /// The operand byte.
    pub fn byte(self) -> u8 {
        let v = match self.volume {
            Some(v) => v.min(100),
            None => Self::UNKNOWN_VOLUME,
        };
        (u8::from(self.muted) << 7) | v
    }

    /// From the operand byte; volumes 0x65 to 0x7e are reserved and read as
    /// unknown.
    pub fn from_byte(b: u8) -> AudioStatus {
        let v = b & 0x7f;
        AudioStatus {
            muted: b & 0x80 != 0,
            volume: (v <= 100).then_some(v),
        }
    }
}

/// The one Short Audio Descriptor the hub reports: LPCM, 2 channels, 48 kHz,
/// 16 bit. Never AC-3, E-AC-3, DTS or anything else (P2 Option A: the hub
/// captures stereo LPCM only, `docs/proposals/P2-theater-scope.md`).
///
/// The bytes follow Android's AudioSystem device, which builds a SAD as "CEC
/// 1.4 table 29" describes: byte 0 bits 3 to 6 the audio format code (LPCM is
/// 1, `Constants.AUDIO_CODEC_LPCM`) and bits 0 to 2 the channel count less
/// one; byte 1 one bit per sample rate in the order 32, 44.1, 48, 88.2, 96,
/// 176.4, 192 kHz (48 kHz is bit 2); byte 2 for LPCM bit 0 = 16 bit
/// (`HdmiCecLocalDeviceAudioSystem.getFirstByteOfSAD`,
/// `getSecondByteOfSAD`, `getSupportedShortAudioDescriptor`, Apache-2.0,
/// read 2026-10-01). So 0x09 0x04 0x01.
pub const LPCM_2CH_48K_16BIT: [u8; 3] = [(1 << 3) | (2 - 1), 1 << 2, 0x01];

/// The audio format code of LPCM in a Short Audio Descriptor request.
pub const AUDIO_FORMAT_LPCM: u8 = 1;

/// One CEC message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    /// The initiator's logical address, 0 to 15.
    pub initiator: u8,
    /// The destination's logical address, 0 to 15 (15 is broadcast).
    pub destination: u8,
    /// The opcode; `None` for a polling message (the header alone).
    pub opcode: Option<u8>,
    /// The operands, at most 14 bytes.
    pub operands: Vec<u8>,
}

/// Why bytes are not a CEC message, or a message cannot be sent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodecError {
    /// No bytes at all.
    Empty,
    /// More than 16 bytes.
    TooLong(usize),
    /// A logical address past 15.
    BadAddress(u8),
    /// Operands with no opcode.
    OperandsWithoutOpcode,
}

impl fmt::Display for CodecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CodecError::Empty => write!(f, "a CEC message has at least its header byte"),
            CodecError::TooLong(n) => write!(f, "a CEC message is at most 16 bytes, not {}", n),
            CodecError::BadAddress(a) => write!(f, "logical address {} is past 15", a),
            CodecError::OperandsWithoutOpcode => write!(f, "operands with no opcode"),
        }
    }
}

impl std::error::Error for CodecError {}

impl Message {
    /// A message with an opcode and operands.
    pub fn new(initiator: u8, destination: u8, opcode: u8, operands: &[u8]) -> Message {
        Message {
            initiator,
            destination,
            opcode: Some(opcode),
            operands: operands.to_vec(),
        }
    }

    /// A polling message: the header alone.
    pub fn poll(initiator: u8, destination: u8) -> Message {
        Message {
            initiator,
            destination,
            opcode: None,
            operands: Vec::new(),
        }
    }

    /// Whether it goes to every device.
    pub fn is_broadcast(&self) -> bool {
        self.destination == BROADCAST
    }

    /// The bytes on the bus.
    pub fn encode(&self) -> Result<Vec<u8>, CodecError> {
        for a in [self.initiator, self.destination] {
            if a > 15 {
                return Err(CodecError::BadAddress(a));
            }
        }
        let mut out = vec![(self.initiator << 4) | self.destination];
        match self.opcode {
            Some(op) => out.push(op),
            None if !self.operands.is_empty() => return Err(CodecError::OperandsWithoutOpcode),
            None => {}
        }
        out.extend_from_slice(&self.operands);
        if out.len() > MAX_MESSAGE_LEN {
            return Err(CodecError::TooLong(out.len()));
        }
        Ok(out)
    }

    /// The message these bytes are.
    pub fn decode(bytes: &[u8]) -> Result<Message, CodecError> {
        let (&header, rest) = bytes.split_first().ok_or(CodecError::Empty)?;
        if bytes.len() > MAX_MESSAGE_LEN {
            return Err(CodecError::TooLong(bytes.len()));
        }
        Ok(Message {
            initiator: header >> 4,
            destination: header & 0x0F,
            opcode: rest.first().copied(),
            operands: rest.get(1..).unwrap_or(&[]).to_vec(),
        })
    }
}

impl fmt::Display for Message {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:x}->{:x}", self.initiator, self.destination)?;
        match self.opcode {
            None => write!(f, " poll"),
            Some(op) => {
                match opcode::name(op) {
                    Some(n) => write!(f, " {}", n)?,
                    None => write!(f, " op=0x{:02x}", op)?,
                }
                if !self.operands.is_empty() {
                    write!(f, " [")?;
                    for (i, b) in self.operands.iter().enumerate() {
                        if i > 0 {
                            write!(f, " ")?;
                        }
                        write!(f, "{:02x}", b)?;
                    }
                    write!(f, "]")?;
                }
                Ok(())
            }
        }
    }
}

/// The messages the Audio System sends, built in one place so the role and
/// the fixtures agree byte for byte (Android's `HdmiCecMessageBuilder`
/// builds the same ones, Apache-2.0, read 2026-10-01).
pub mod build {
    use super::*;

    /// `<Feature Abort>` [`about`][`reason`].
    pub fn feature_abort(from: u8, to: u8, about: u8, reason: AbortReason) -> Message {
        Message::new(from, to, opcode::FEATURE_ABORT, &[about, reason.byte()])
    }

    /// `<Report Audio Status>`.
    pub fn report_audio_status(from: u8, to: u8, status: AudioStatus) -> Message {
        Message::new(from, to, opcode::REPORT_AUDIO_STATUS, &[status.byte()])
    }

    /// `<Set System Audio Mode>`, to `to` (broadcast or the TV).
    pub fn set_system_audio_mode(from: u8, to: u8, on: bool) -> Message {
        Message::new(from, to, opcode::SET_SYSTEM_AUDIO_MODE, &[u8::from(on)])
    }

    /// `<System Audio Mode Status>`.
    pub fn system_audio_mode_status(from: u8, to: u8, on: bool) -> Message {
        Message::new(from, to, opcode::SYSTEM_AUDIO_MODE_STATUS, &[u8::from(on)])
    }

    /// `<Report Physical Address>`, broadcast.
    pub fn report_physical_address(from: u8, pa: PhysicalAddress, device_type: u8) -> Message {
        let [hi, lo] = pa.bytes();
        Message::new(
            from,
            BROADCAST,
            opcode::REPORT_PHYSICAL_ADDRESS,
            &[hi, lo, device_type],
        )
    }

    /// `<Report Power Status>`.
    pub fn report_power_status(from: u8, to: u8, status: PowerStatus) -> Message {
        Message::new(from, to, opcode::REPORT_POWER_STATUS, &[status as u8])
    }

    /// `<Set OSD Name>`; the name must already be 1 to 14 printable ASCII
    /// bytes (`crate::role::Config::check`).
    pub fn set_osd_name(from: u8, to: u8, name: &str) -> Message {
        Message::new(from, to, opcode::SET_OSD_NAME, name.as_bytes())
    }

    /// `<CEC Version>`.
    pub fn cec_version(from: u8, to: u8, version: u8) -> Message {
        Message::new(from, to, opcode::CEC_VERSION, &[version])
    }

    /// `<Device Vendor ID>`, broadcast, the OUI most significant byte first.
    pub fn device_vendor_id(from: u8, oui: u32) -> Message {
        let b = oui.to_be_bytes();
        Message::new(from, BROADCAST, opcode::DEVICE_VENDOR_ID, &b[1..])
    }

    /// `<Report Short Audio Descriptor>`.
    pub fn report_short_audio_descriptor(from: u8, to: u8, sads: &[[u8; 3]]) -> Message {
        let bytes: Vec<u8> = sads.iter().flatten().copied().collect();
        Message::new(from, to, opcode::REPORT_SHORT_AUDIO_DESCRIPTOR, &bytes)
    }

    /// `<Give Device Power Status>`.
    pub fn give_device_power_status(from: u8, to: u8) -> Message {
        Message::new(from, to, opcode::GIVE_DEVICE_POWER_STATUS, &[])
    }

    /// `<Initiate ARC>`.
    pub fn initiate_arc(from: u8, to: u8) -> Message {
        Message::new(from, to, opcode::INITIATE_ARC, &[])
    }

    /// `<Terminate ARC>`.
    pub fn terminate_arc(from: u8, to: u8) -> Message {
        Message::new(from, to, opcode::TERMINATE_ARC, &[])
    }

    /// `<User Control Pressed>` [`key`], as a TV sends it.
    pub fn user_control_pressed(from: u8, to: u8, key: u8) -> Message {
        Message::new(from, to, opcode::USER_CONTROL_PRESSED, &[key])
    }

    /// `<User Control Released>`.
    pub fn user_control_released(from: u8, to: u8) -> Message {
        Message::new(from, to, opcode::USER_CONTROL_RELEASED, &[])
    }

    /// `<System Audio Mode Request>`: on for the path at `pa`, or off
    /// (`None`, no operand).
    pub fn system_audio_mode_request(from: u8, to: u8, pa: Option<PhysicalAddress>) -> Message {
        let operands = pa.map(|p| p.bytes().to_vec()).unwrap_or_default();
        Message::new(from, to, opcode::SYSTEM_AUDIO_MODE_REQUEST, &operands)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_header_carries_both_addresses_and_a_poll_is_the_header_alone() {
        let m = build::report_audio_status(
            AUDIO_SYSTEM,
            TV,
            AudioStatus {
                muted: true,
                volume: Some(50),
            },
        );
        assert_eq!(m.encode().unwrap(), vec![0x50, 0x7a, 0xb2]);
        assert_eq!(Message::decode(&[0x50, 0x7a, 0xb2]).unwrap(), m);
        let p = Message::poll(5, 5);
        assert_eq!(p.encode().unwrap(), vec![0x55]);
        assert_eq!(Message::decode(&[0x55]).unwrap(), p);
        assert_eq!(Message::decode(&[]), Err(CodecError::Empty));
        assert_eq!(Message::decode(&[0; 17]), Err(CodecError::TooLong(17)));
        let long = Message::new(5, 0, 0x47, &[b'a'; 15]);
        assert_eq!(long.encode(), Err(CodecError::TooLong(17)));
    }

    #[test]
    fn audio_status_keeps_mute_and_volume_apart() {
        for v in [0u8, 1, 46, 50, 54, 99, 100] {
            for muted in [false, true] {
                let s = AudioStatus {
                    muted,
                    volume: Some(v),
                };
                assert_eq!(AudioStatus::from_byte(s.byte()), s);
            }
        }
        let unknown = AudioStatus {
            muted: false,
            volume: None,
        };
        assert_eq!(unknown.byte(), 0x7f);
        assert_eq!(AudioStatus::from_byte(0x70).volume, None, "reserved");
    }

    #[test]
    fn physical_addresses_print_as_nibbles_and_holes_are_invalid() {
        assert_eq!(PhysicalAddress(0x1000).to_string(), "1.0.0.0");
        assert_eq!(PhysicalAddress(0x2130).to_string(), "2.1.3.0");
        assert!(PhysicalAddress(0x1000).is_valid());
        assert!(PhysicalAddress::TV.is_valid());
        assert!(
            !PhysicalAddress(0x1010).is_valid(),
            "0 then a nonzero nibble"
        );
        assert!(!PhysicalAddress(0x0100).is_valid());
    }

    #[test]
    fn the_one_short_audio_descriptor_is_lpcm_two_channels_48k_16_bit() {
        assert_eq!(LPCM_2CH_48K_16BIT, [0x09, 0x04, 0x01]);
        assert_eq!(LPCM_2CH_48K_16BIT[0] >> 3, AUDIO_FORMAT_LPCM);
        assert_eq!((LPCM_2CH_48K_16BIT[0] & 7) + 1, 2, "two channels");
    }

    #[test]
    fn every_named_opcode_has_one_name() {
        for (op, name) in opcode::NAMES {
            assert_eq!(opcode::from_name(name), Some(op));
            assert_eq!(opcode::name(op), Some(name));
        }
    }
}
