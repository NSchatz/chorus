//! RenderingControl:1: Master volume and mute.
//!
//! RCS1. chorus offers one logical channel, `Master` (RCS1 section 2.2.19),
//! with a volume of 0 to 100 in steps of 1 (section 2.2.16: "from a minimum
//! of 0 to some device specific maximum", 0 being silence) and a mute.
//!
//! ## The mapping to chorus's volume
//!
//! chorus's control plane holds a volume in thousandths, 0 to 1000, an
//! amplitude factor. The mapping is linear: UPnP volume `v` is `v * 10`
//! thousandths ([`to_thousandths`]), and back is `(t + 5) / 10`, that is,
//! rounded to the nearest with halves up ([`from_thousandths`]). Every UPnP
//! value survives the round trip exactly; a value set elsewhere in finer
//! steps (375 thousandths) is reported as the nearest UPnP step (38).
//!
//! ## Limits: the real value is what is reported
//!
//! A room can have a volume limit. `SetVolume` above it is not an error
//! (RCS1 defines none, and a control point shows errors badly): the action
//! succeeds, the server applies the request through the control plane, which
//! clamps it, and then tells this state the value that actually holds
//! ([`RenderingControl::report`]). `GetVolume` and the LastChange event say
//! that real value, so a slider dragged past the limit settles back onto it.
//! For that reason an action never changes the variables here by itself:
//! only `report` does, and a change made from anywhere else (chorus's own
//! app, another control point) reaches subscribers the same way.

use crate::lastchange::{Change, Moderator};
use crate::soap::Invocation;
use crate::{error, instance_is_zero, Outputs, UpnpError};

/// The highest UPnP volume.
pub const MAX_VOLUME: u16 = 100;

/// The one preset: RCS1 section 2.2.2 and 2.2.21 require the list to hold at
/// least "FactoryDefaults".
pub const PRESET: &str = "FactoryDefaults";

/// UPnP volume to chorus thousandths: `v * 10`, with `v` above 100 taken as
/// 100.
pub fn to_thousandths(volume: u16) -> u16 {
    volume.min(MAX_VOLUME) * 10
}

/// chorus thousandths to UPnP volume: `(t + 5) / 10`, nearest with halves
/// up, with `t` above 1000 taken as 1000.
pub fn from_thousandths(thousandths: u16) -> u16 {
    (thousandths.min(1000) + 5) / 10
}

/// What the server must do after a RenderingControl action.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect {
    /// Set the target's volume through the control plane (which applies the
    /// room's limit), then [`RenderingControl::report`] what holds.
    SetVolume {
        /// The requested volume, in thousandths.
        thousandths: u16,
    },
    /// Set the target's mute, then report what holds.
    SetMute {
        /// The requested mute.
        mute: bool,
    },
}

/// The RenderingControl state of one renderer.
#[derive(Clone, Debug)]
pub struct RenderingControl {
    volume: u16,
    mute: bool,
    events: Moderator,
}

impl RenderingControl {
    /// The state for a target whose volume and mute are these now.
    pub fn new(thousandths: u16, mute: bool) -> RenderingControl {
        RenderingControl {
            volume: from_thousandths(thousandths),
            mute,
            events: Moderator::new(),
        }
    }

    /// The UPnP volume, 0 to 100.
    pub fn volume(&self) -> u16 {
        self.volume
    }

    /// The mute.
    pub fn mute(&self) -> bool {
        self.mute
    }

    /// The moderation queue the changes are recorded in; the server takes
    /// events from it ([`Moderator::take`]).
    pub fn events(&mut self) -> &mut Moderator {
        &mut self.events
    }

    /// Tells the state what the target's volume and mute really are: after
    /// an action's effect was applied, and whenever they change by any other
    /// path. A changed variable is recorded for LastChange with
    /// `channel="Master"`; an unchanged one is not.
    pub fn report(&mut self, thousandths: u16, mute: bool) {
        let volume = from_thousandths(thousandths);
        if volume != self.volume {
            self.volume = volume;
            self.events
                .record(Change::master("Volume", volume.to_string()));
        }
        if mute != self.mute {
            self.mute = mute;
            self.events
                .record(Change::master("Mute", if mute { "1" } else { "0" }));
        }
    }

    /// Every indirectly evented variable with its current value, for the
    /// initial event of a subscription (RCS1 section 2.3.1: "the device
    /// should respond with the current values of all (indirectly evented)
    /// state variables").
    pub fn evented(&self) -> Vec<Change> {
        vec![
            Change::new("PresetNameList", PRESET),
            Change::master("Mute", if self.mute { "1" } else { "0" }),
            Change::master("Volume", self.volume.to_string()),
        ]
    }

    /// Performs a RenderingControl action that passed
    /// [`crate::soap::validate`]. In order:
    ///
    /// - `InstanceID` other than 0: 702 Invalid InstanceID (RCS1 section
    ///   2.4.1.3; note AVTransport numbers this 718);
    /// - `Channel` other than `Master`: 402 Invalid Args. RCS1 has no code
    ///   for a channel the device lacks; the action tables (sections
    ///   2.4.27.3 to 2.4.30.3) list only 402, 501 and 702, and `Master` is
    ///   the only value in the SCPD's allowed list;
    /// - `DesiredVolume` that is not an integer of 0 to 100, `DesiredMute`
    ///   that is not a boolean (`1`, `0`, `true`, `false`, `yes`, `no`, the
    ///   forms of UDA11 section 2.5): 402;
    /// - `SelectPreset` with a name other than `FactoryDefaults`: 701
    ///   Invalid Name (RCS1 section 2.4.2.3). `FactoryDefaults` itself
    ///   succeeds and changes nothing: chorus has no factory volume to
    ///   restore, and a preset that moved a room's volume by surprise would
    ///   be a loudness jump nobody asked for.
    pub fn invoke(&mut self, invocation: &Invocation) -> Result<(Outputs, Vec<Effect>), UpnpError> {
        if !instance_is_zero(invocation.input("InstanceID"))? {
            return Err(error::RCS_INVALID_INSTANCE_ID);
        }
        let master = |inv: &Invocation| -> Result<(), UpnpError> {
            if inv.input("Channel").trim() == "Master" {
                Ok(())
            } else {
                Err(error::INVALID_ARGS)
            }
        };
        let none = Vec::new;
        match invocation.action.name {
            "ListPresets" => Ok((vec![("CurrentPresetNameList", PRESET.to_string())], none())),
            "SelectPreset" => {
                if invocation.input("PresetName").trim() == PRESET {
                    Ok((Vec::new(), none()))
                } else {
                    Err(error::RCS_INVALID_NAME)
                }
            }
            "GetMute" => {
                master(invocation)?;
                Ok((
                    vec![("CurrentMute", if self.mute { "1" } else { "0" }.to_string())],
                    none(),
                ))
            }
            "SetMute" => {
                master(invocation)?;
                let mute = match invocation
                    .input("DesiredMute")
                    .trim()
                    .to_ascii_lowercase()
                    .as_str()
                {
                    "1" | "true" | "yes" => true,
                    "0" | "false" | "no" => false,
                    _ => return Err(error::INVALID_ARGS),
                };
                Ok((Vec::new(), vec![Effect::SetMute { mute }]))
            }
            "GetVolume" => {
                master(invocation)?;
                Ok((vec![("CurrentVolume", self.volume.to_string())], none()))
            }
            "SetVolume" => {
                master(invocation)?;
                let text = invocation.input("DesiredVolume").trim();
                let volume = if !text.is_empty()
                    && text.len() <= 5
                    && text.bytes().all(|b| b.is_ascii_digit())
                {
                    text.parse::<u16>().ok()
                } else {
                    None
                }
                .filter(|v| *v <= MAX_VOLUME)
                .ok_or(error::INVALID_ARGS)?;
                Ok((
                    Vec::new(),
                    vec![Effect::SetVolume {
                        thousandths: to_thousandths(volume),
                    }],
                ))
            }
            _ => Err(error::INVALID_ACTION),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lastchange::{event_xml, RCS_NS};
    use crate::soap::{validate, ActionRequest};
    use crate::Service;

    fn call(
        rcs: &mut RenderingControl,
        action: &str,
        args: &[(&str, &str)],
    ) -> Result<(Outputs, Vec<Effect>), UpnpError> {
        let request = ActionRequest {
            service_type: Service::RenderingControl.service_type().into(),
            action: action.into(),
            arguments: args
                .iter()
                .map(|(n, v)| (n.to_string(), v.to_string()))
                .collect(),
        };
        rcs.invoke(&validate(Service::RenderingControl, &request)?)
    }

    #[test]
    fn the_mapping_is_linear_and_every_upnp_step_round_trips() {
        for v in 0..=100u16 {
            assert_eq!(to_thousandths(v), v * 10);
            assert_eq!(from_thousandths(to_thousandths(v)), v);
        }
        assert_eq!(to_thousandths(250), 1000);
        // Finer values from elsewhere: nearest, halves up.
        for (t, v) in [
            (0, 0),
            (4, 0),
            (5, 1),
            (14, 1),
            (15, 2),
            (374, 37),
            (375, 38),
            (994, 99),
            (995, 100),
            (1000, 100),
            (5000, 100),
        ] {
            assert_eq!(from_thousandths(t), v, "{t}");
        }
    }

    #[test]
    fn get_and_set_go_through_the_server_and_report_the_real_value() {
        let mut rcs = RenderingControl::new(300, false);
        let id = ("InstanceID", "0");
        let ch = ("Channel", "Master");
        assert_eq!(
            call(&mut rcs, "GetVolume", &[id, ch]).unwrap().0,
            [("CurrentVolume", "30".to_string())]
        );
        // SetVolume asks; it does not change the variable.
        let (out, effects) =
            call(&mut rcs, "SetVolume", &[id, ch, ("DesiredVolume", "80")]).unwrap();
        assert!(out.is_empty());
        assert_eq!(effects, [Effect::SetVolume { thousandths: 800 }]);
        assert_eq!(rcs.volume(), 30);
        assert!(!rcs.events().is_pending());
        // The room's limit is 600: the server reports what holds.
        rcs.report(600, false);
        assert_eq!(
            call(&mut rcs, "GetVolume", &[id, ch]).unwrap().0,
            [("CurrentVolume", "60".to_string())]
        );
        let changes = rcs.events().take(0).unwrap();
        assert_eq!(
            event_xml(RCS_NS, &changes),
            "<Event xmlns=\"urn:schemas-upnp-org:metadata-1-0/RCS/\"><InstanceID val=\"0\"><Volume channel=\"Master\" val=\"60\"/></InstanceID></Event>"
        );
        // Mute.
        let (_, effects) = call(&mut rcs, "SetMute", &[id, ch, ("DesiredMute", "1")]).unwrap();
        assert_eq!(effects, [Effect::SetMute { mute: true }]);
        rcs.report(600, true);
        assert_eq!(
            call(&mut rcs, "GetMute", &[id, ch]).unwrap().0,
            [("CurrentMute", "1".to_string())]
        );
        assert_eq!(rcs.events().pending(), [Change::master("Mute", "1")]);
        // Reporting the same values again records nothing new.
        rcs.events().discard();
        rcs.report(600, true);
        rcs.report(604, true);
        assert!(!rcs.events().is_pending());
        assert!(rcs.mute());
    }

    #[test]
    fn booleans_are_read_in_every_uda_form() {
        let mut rcs = RenderingControl::new(0, false);
        let id = ("InstanceID", "0");
        let ch = ("Channel", "Master");
        for (text, expect) in [
            ("1", true),
            ("true", true),
            ("YES", true),
            ("0", false),
            ("False", false),
            ("no", false),
        ] {
            let (_, e) = call(&mut rcs, "SetMute", &[id, ch, ("DesiredMute", text)]).unwrap();
            assert_eq!(e, [Effect::SetMute { mute: expect }], "{text}");
        }
        for bad in ["", "2", "on", "maybe"] {
            assert_eq!(
                call(&mut rcs, "SetMute", &[id, ch, ("DesiredMute", bad)]),
                Err(error::INVALID_ARGS),
                "{bad}"
            );
        }
    }

    #[test]
    fn the_errors_are_the_specifications() {
        let mut rcs = RenderingControl::new(500, false);
        let id = ("InstanceID", "0");
        let ch = ("Channel", "Master");
        // 702 for another instance, on every action.
        for (action, rest) in [
            ("ListPresets", &[][..]),
            ("SelectPreset", &[("PresetName", "FactoryDefaults")][..]),
            ("GetMute", &[ch][..]),
            ("SetMute", &[ch, ("DesiredMute", "1")][..]),
            ("GetVolume", &[ch][..]),
            ("SetVolume", &[ch, ("DesiredVolume", "1")][..]),
        ] {
            let mut args = vec![("InstanceID", "7")];
            args.extend_from_slice(rest);
            assert_eq!(
                call(&mut rcs, action, &args),
                Err(error::RCS_INVALID_INSTANCE_ID),
                "{action}"
            );
            args[0] = ("InstanceID", "x");
            assert_eq!(
                call(&mut rcs, action, &args),
                Err(error::INVALID_ARGS),
                "{action}"
            );
        }
        assert_eq!(error::RCS_INVALID_INSTANCE_ID.code, 702);
        // 402 for a channel other than Master.
        for channel in ["LF", "RF", "master", ""] {
            let c = ("Channel", channel);
            assert_eq!(
                call(&mut rcs, "GetVolume", &[id, c]),
                Err(error::INVALID_ARGS),
                "{channel}"
            );
            assert_eq!(
                call(&mut rcs, "GetMute", &[id, c]),
                Err(error::INVALID_ARGS)
            );
            assert_eq!(
                call(&mut rcs, "SetVolume", &[id, c, ("DesiredVolume", "1")]),
                Err(error::INVALID_ARGS)
            );
            assert_eq!(
                call(&mut rcs, "SetMute", &[id, c, ("DesiredMute", "1")]),
                Err(error::INVALID_ARGS)
            );
        }
        // 402 for a volume that is not 0 to 100.
        for bad in ["101", "-1", "1.5", "", "loud", "65536", "999999"] {
            assert_eq!(
                call(&mut rcs, "SetVolume", &[id, ch, ("DesiredVolume", bad)]),
                Err(error::INVALID_ARGS),
                "{bad}"
            );
        }
        for good in ["0", "100", " 50 "] {
            assert!(call(&mut rcs, "SetVolume", &[id, ch, ("DesiredVolume", good)]).is_ok());
        }
        // Presets.
        assert_eq!(
            call(&mut rcs, "ListPresets", &[id]).unwrap().0,
            [("CurrentPresetNameList", "FactoryDefaults".to_string())]
        );
        let (out, effects) = call(
            &mut rcs,
            "SelectPreset",
            &[id, ("PresetName", "FactoryDefaults")],
        )
        .unwrap();
        assert!(out.is_empty() && effects.is_empty());
        assert_eq!(
            call(&mut rcs, "SelectPreset", &[id, ("PresetName", "Nope")]),
            Err(error::RCS_INVALID_NAME)
        );
        assert_eq!(error::RCS_INVALID_NAME.code, 701);
        // Not offered at all.
        assert_eq!(
            call(&mut rcs, "GetVolumeDB", &[id, ch]),
            Err(error::INVALID_ACTION)
        );
        // No error changed anything.
        assert_eq!(rcs.volume(), 50);
        assert!(!rcs.events().is_pending());
    }

    #[test]
    fn the_initial_event_holds_every_variable() {
        let rcs = RenderingControl::new(200, true);
        assert_eq!(
            event_xml(RCS_NS, &rcs.evented()),
            "<Event xmlns=\"urn:schemas-upnp-org:metadata-1-0/RCS/\"><InstanceID val=\"0\"><PresetNameList val=\"FactoryDefaults\"/><Mute channel=\"Master\" val=\"1\"/><Volume channel=\"Master\" val=\"20\"/></InstanceID></Event>"
        );
    }
}
