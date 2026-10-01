//! Whether a received message is well formed for its opcode: who may send
//! it, where it may go, and how long its operands are.
//!
//! The rules are Android's `HdmiCecMessageValidator` (Apache-2.0,
//! <https://android.googlesource.com/platform/frameworks/base/+/refs/heads/main/services/core/java/com/android/server/hdmi/HdmiCecMessageValidator.java>,
//! read 2026-10-01), restated here for the opcodes chorus names, not copied:
//! each opcode has a set of valid initiators, a set of valid destinations
//! (directly addressed, broadcast, or either) and a parameter check. What the
//! caller does with each outcome follows Android's `HdmiControlService.
//! handleCecCommand` and `HdmiCecController.onReceiveCommand` (same tree,
//! read 2026-10-01):
//!
//! - [`Validity::Parameter`] and [`Validity::ParameterLong`]: answered with
//!   `<Feature Abort>` ["Invalid operand"] when directly addressed;
//! - [`Validity::Source`], [`Validity::Destination`] and
//!   [`Validity::ParameterShort`]: dropped without an answer. So a directly
//!   addressed message received as a broadcast is ignored (CTS 12-2,
//!   `HdmiCecInvalidMessagesTest`), and so is a broadcast-only one received
//!   directly addressed.
//!
//! An operand list longer than the minimum is accepted, as Android accepts
//! it ("the parameter can be extended in the future version"), except where
//! Android's check is a single byte in a range, which also refuses a longer
//! one.

use crate::codec::{opcode, Message, PhysicalAddress, AUDIO_SYSTEM, BROADCAST, UNREGISTERED};

/// The outcome of [`validate`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Validity {
    /// Well formed.
    Ok,
    /// The initiator may not send this opcode.
    Source,
    /// It may not go to this destination (direct versus broadcast).
    Destination,
    /// An operand is out of range or malformed.
    Parameter,
    /// Too few operand bytes.
    ParameterShort,
    /// More operand bytes than a fixed one-byte operand allows.
    ParameterLong,
}

impl Validity {
    /// The fixture spelling.
    pub fn name(self) -> &'static str {
        match self {
            Validity::Ok => "ok",
            Validity::Source => "source",
            Validity::Destination => "destination",
            Validity::Parameter => "parameter",
            Validity::ParameterShort => "parameter_short",
            Validity::ParameterLong => "parameter_long",
        }
    }

    /// The validity spelled `name`.
    pub fn from_name(name: &str) -> Option<Validity> {
        [
            Validity::Ok,
            Validity::Source,
            Validity::Destination,
            Validity::Parameter,
            Validity::ParameterShort,
            Validity::ParameterLong,
        ]
        .into_iter()
        .find(|v| v.name() == name)
    }
}

/// Who may send an opcode.
#[derive(Clone, Copy)]
enum Sender {
    /// Any address, Unregistered included.
    All,
    /// Any but Unregistered.
    Registered,
    /// Any but the Audio System (Active Source: an Audio System is never one).
    NotAudioSystem,
    /// Only the Audio System.
    AudioSystem,
}

/// Where an opcode may go.
#[derive(Clone, Copy)]
enum To {
    /// One device.
    Direct,
    /// Everyone.
    Broadcast,
    /// Either.
    Either,
}

/// What its operands must be.
#[derive(Clone, Copy)]
enum Params {
    /// At least `n` bytes.
    AtLeast(usize),
    /// A physical address, well formed.
    Address,
    /// None, or a physical address (System Audio Mode Request).
    OptionalAddress,
    /// A physical address and a device type 0..=7 but not 2.
    ReportPhysicalAddress,
    /// Two physical addresses.
    TwoAddresses,
    /// Exactly one byte, in `lo..=hi`.
    OneByte(u8, u8),
    /// At least one byte, the first in `lo..=hi`.
    FirstByte(u8, u8),
    /// 1 to 14 printable ASCII bytes.
    Ascii,
}

fn rule(op: u8) -> Option<(Sender, To, Params)> {
    use opcode::*;
    use Params::*;
    use Sender::*;
    use To::*;
    Some(match op {
        ACTIVE_SOURCE => (NotAudioSystem, Broadcast, Address),
        REPORT_PHYSICAL_ADDRESS => (All, Broadcast, ReportPhysicalAddress),
        ROUTING_CHANGE => (All, Broadcast, TwoAddresses),
        ROUTING_INFORMATION => (All, Broadcast, Address),
        SET_STREAM_PATH => (Registered, Broadcast, Address),
        SYSTEM_AUDIO_MODE_REQUEST => (Registered, Direct, OptionalAddress),
        ABORT
        | GET_CEC_VERSION
        | GIVE_AUDIO_STATUS
        | GIVE_DEVICE_POWER_STATUS
        | GIVE_OSD_NAME
        | GIVE_SYSTEM_AUDIO_MODE_STATUS
        | IMAGE_VIEW_ON
        | TEXT_VIEW_ON
        | INITIATE_ARC
        | TERMINATE_ARC
        | REPORT_ARC_INITIATED
        | REPORT_ARC_TERMINATED
        | REQUEST_ARC_INITIATION
        | REQUEST_ARC_TERMINATION
        | USER_CONTROL_RELEASED => (Registered, Direct, AtLeast(0)),
        GIVE_DEVICE_VENDOR_ID | GIVE_PHYSICAL_ADDRESS | GIVE_FEATURES => (All, Direct, AtLeast(0)),
        REQUEST_ACTIVE_SOURCE => (All, Broadcast, AtLeast(0)),
        STANDBY => (All, Either, AtLeast(0)),
        CEC_VERSION => (Registered, Direct, AtLeast(1)),
        DEVICE_VENDOR_ID => (Registered, Broadcast, AtLeast(3)),
        VENDOR_COMMAND => (All, Direct, AtLeast(1)),
        VENDOR_COMMAND_WITH_ID => (All, Either, AtLeast(4)),
        SET_OSD_NAME => (Registered, Direct, Ascii),
        USER_CONTROL_PRESSED => (Registered, Direct, AtLeast(1)),
        REPORT_POWER_STATUS => (Registered, Either, FirstByte(0, 3)),
        FEATURE_ABORT => (Registered, Direct, AtLeast(2)),
        REPORT_AUDIO_STATUS => (Registered, Direct, AtLeast(1)),
        REPORT_SHORT_AUDIO_DESCRIPTOR => (Registered, Direct, AtLeast(3)),
        REQUEST_SHORT_AUDIO_DESCRIPTOR => (Registered, Direct, AtLeast(1)),
        SET_SYSTEM_AUDIO_MODE => (AudioSystem, Either, OneByte(0, 1)),
        SYSTEM_AUDIO_MODE_STATUS => (Registered, Direct, OneByte(0, 1)),
        _ => return None,
    })
}

fn address_at(p: &[u8], at: usize) -> bool {
    PhysicalAddress::from_bytes(p[at], p[at + 1]).is_valid()
}

fn check(params: Params, p: &[u8]) -> Validity {
    let ok = |b: bool| if b { Validity::Ok } else { Validity::Parameter };
    match params {
        Params::AtLeast(n) => {
            if p.len() < n {
                Validity::ParameterShort
            } else {
                Validity::Ok
            }
        }
        Params::Address => {
            if p.len() < 2 {
                Validity::ParameterShort
            } else {
                ok(address_at(p, 0))
            }
        }
        Params::OptionalAddress => {
            if p.is_empty() {
                Validity::Ok
            } else {
                check(Params::Address, p)
            }
        }
        Params::ReportPhysicalAddress => {
            if p.len() < 3 {
                Validity::ParameterShort
            } else {
                // Device types 0 to 7 with 2 reserved (Android's
                // `isValidType`: DEVICE_TV 0 to DEVICE_VIDEO_PROCESSOR 7,
                // not DEVICE_RESERVED 2).
                ok(address_at(p, 0) && p[2] <= 7 && p[2] != 2)
            }
        }
        Params::TwoAddresses => {
            if p.len() < 4 {
                Validity::ParameterShort
            } else {
                ok(address_at(p, 0) && address_at(p, 2))
            }
        }
        Params::OneByte(lo, hi) => match p.len() {
            0 => Validity::ParameterShort,
            1 => ok((lo..=hi).contains(&p[0])),
            _ => Validity::ParameterLong,
        },
        Params::FirstByte(lo, hi) => {
            if p.is_empty() {
                Validity::ParameterShort
            } else {
                ok((lo..=hi).contains(&p[0]))
            }
        }
        Params::Ascii => {
            if p.is_empty() {
                Validity::ParameterShort
            } else {
                ok(p.iter().take(14).all(|b| (0x20..=0x7e).contains(b)))
            }
        }
    }
}

/// Check `m` against its opcode's rule. A polling message, and an opcode
/// with no rule here, is [`Validity::Ok`]: the role decides what to do with
/// an opcode it does not know (`<Feature Abort>` if directly addressed).
pub fn validate(m: &Message) -> Validity {
    let Some(op) = m.opcode else {
        return Validity::Ok;
    };
    let Some((from, to, params)) = rule(op) else {
        return Validity::Ok;
    };
    let source_ok = match from {
        Sender::All => true,
        Sender::Registered => m.initiator != UNREGISTERED,
        Sender::NotAudioSystem => m.initiator != AUDIO_SYSTEM,
        Sender::AudioSystem => m.initiator == AUDIO_SYSTEM,
    };
    if !source_ok {
        return Validity::Source;
    }
    let broadcast = m.destination == BROADCAST;
    let destination_ok = match to {
        To::Direct => !broadcast,
        To::Broadcast => broadcast,
        To::Either => true,
    };
    if !destination_ok {
        return Validity::Destination;
    }
    check(params, &m.operands)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::TV;

    #[test]
    fn directly_addressed_messages_received_as_broadcast_are_destination_errors() {
        // CTS 12-2: each of these, broadcast, must be ignored.
        for op in [
            opcode::GIVE_AUDIO_STATUS,
            opcode::GIVE_SYSTEM_AUDIO_MODE_STATUS,
            opcode::REQUEST_ARC_INITIATION,
            opcode::REQUEST_ARC_TERMINATION,
        ] {
            assert_eq!(
                validate(&Message::new(TV, BROADCAST, op, &[])),
                Validity::Destination
            );
        }
        let sad = Message::new(TV, BROADCAST, opcode::REQUEST_SHORT_AUDIO_DESCRIPTOR, &[1]);
        assert_eq!(validate(&sad), Validity::Destination);
        let samr = Message::new(TV, BROADCAST, opcode::SYSTEM_AUDIO_MODE_REQUEST, &[0, 0]);
        assert_eq!(validate(&samr), Validity::Destination);
    }

    #[test]
    fn broadcast_only_messages_received_directly_are_destination_errors() {
        let m = Message::new(TV, AUDIO_SYSTEM, opcode::ACTIVE_SOURCE, &[0x10, 0x00]);
        assert_eq!(validate(&m), Validity::Destination);
    }

    #[test]
    fn operand_rules() {
        let short = Message::new(TV, AUDIO_SYSTEM, opcode::FEATURE_ABORT, &[0x72]);
        assert_eq!(validate(&short), Validity::ParameterShort);
        let hole = Message::new(
            TV,
            AUDIO_SYSTEM,
            opcode::SYSTEM_AUDIO_MODE_REQUEST,
            &[0x01, 0x10],
        );
        assert_eq!(validate(&hole), Validity::Parameter);
        let off = Message::new(TV, AUDIO_SYSTEM, opcode::SYSTEM_AUDIO_MODE_REQUEST, &[]);
        assert_eq!(validate(&off), Validity::Ok);
        let long = Message::new(TV, AUDIO_SYSTEM, opcode::SYSTEM_AUDIO_MODE_STATUS, &[1, 0]);
        assert_eq!(validate(&long), Validity::ParameterLong);
        let tv_sets = Message::new(TV, AUDIO_SYSTEM, opcode::SET_SYSTEM_AUDIO_MODE, &[0]);
        assert_eq!(
            validate(&tv_sets),
            Validity::Source,
            "only an Audio System sends it"
        );
    }
}
