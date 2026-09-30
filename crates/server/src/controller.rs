//! The controller role (K65): what a speaker's button asks for, turned into
//! the change the control plane would make.
//!
//! A `controller_command` (docs/protocol.md, "0x32 controller command") is a
//! request from a button or a wall remote, never a second control plane: the
//! volume, mute, join and leave it asks for become the control catalog's own
//! [`Command`], applied by [`Zones::apply`] like any other, and so held to the
//! same checks. The transport commands (play, pause, toggle, next, previous)
//! act on an input, and chorus's inputs arrive in goals 16 and 17; until then
//! they come back as a [`TransportRequest`] for the caller to route, not as a
//! change to the zones. docs/decisions/0063-* records the mapping.

use chorus_control::{Command, Refusal, Volume, Zones};
use chorus_protocol::v2::{self, ControllerCommand};

/// Controller volume is a percentage (0 to 100); the control plane's is
/// thousandths of full scale. ASSUMED linear: one point is ten thousandths,
/// until the volume model of goal 11 gives the curve.
pub const THOUSANDTHS_PER_POINT: i64 = 10;

/// What a transport command asks of the input a room is playing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportRequest {
    /// Start playback.
    Play,
    /// Pause playback.
    Pause,
    /// Play if paused, pause if playing.
    Toggle,
    /// The input's next item.
    Next,
    /// The input's previous item.
    Previous,
}

/// What one controller command becomes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ControllerAction {
    /// A control-plane change, for [`Zones::apply`].
    Apply(Command),
    /// A request for the room's input.
    Transport(TransportRequest),
}

/// Turn `command`, from an endpoint playing `zone`, into what it asks for.
///
/// Every volume is clamped into the catalog's range here, and the result goes
/// through [`Zones::apply`], which refuses an unknown zone; the room limits of
/// K81 (goal 11) apply there too once they exist, so a button is never a
/// bypass (I10).
pub fn translate(
    zones: &Zones,
    zone: &str,
    command: &ControllerCommand,
) -> Result<ControllerAction, Refusal> {
    let current = zones.zone(zone).ok_or_else(|| {
        Refusal::rejected(
            "zone",
            format!("a controller on zone '{zone}' asked for something, and there is no such zone"),
        )
    })?;
    let volume = |thousandths: i64| {
        let clamped = thousandths.clamp(0, i64::from(chorus_control::catalog::VOLUME_SCALE));
        Volume::from_thousandths(clamped).expect("clamped into the catalog's range")
    };
    let zone = zone.to_string();
    Ok(match command.command {
        v2::Command::Play => ControllerAction::Transport(TransportRequest::Play),
        v2::Command::Pause => ControllerAction::Transport(TransportRequest::Pause),
        v2::Command::Toggle => ControllerAction::Transport(TransportRequest::Toggle),
        v2::Command::Next => ControllerAction::Transport(TransportRequest::Next),
        v2::Command::Previous => ControllerAction::Transport(TransportRequest::Previous),
        v2::Command::VolumeSet => ControllerAction::Apply(Command::Volume {
            zone,
            volume: volume(i64::from(command.value) * THOUSANDTHS_PER_POINT),
        }),
        v2::Command::VolumeStep => ControllerAction::Apply(Command::Volume {
            zone,
            volume: volume(
                i64::from(current.volume.thousandths())
                    + i64::from(command.value) * THOUSANDTHS_PER_POINT,
            ),
        }),
        v2::Command::MuteSet => ControllerAction::Apply(Command::Mute {
            zone,
            muted: command.value == 1,
        }),
        v2::Command::Join => {
            if !chorus_control::catalog::is_identifier(&command.target) {
                return Err(Refusal::rejected(
                    "target",
                    format!(
                        "a controller asked to join '{}', which is not a group name",
                        command.target
                    ),
                ));
            }
            ControllerAction::Apply(Command::Group {
                zone,
                group: command.target.clone(),
            })
        }
        v2::Command::Leave => ControllerAction::Apply(Command::Ungroup { zone }),
    })
}
