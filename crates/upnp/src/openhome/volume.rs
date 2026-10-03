//! OpenHome Volume:2: volume, mute and the volume limit.
//!
//! ohP `OpenHome/Av/ProviderVolume.cpp` and `VolumeManager.cpp` at
//! `cccd06dd`. The scale is the one RenderingControl announces: 0 to 100 in
//! steps of 1, the room's volume in hundredths ([`crate::rendering`]). Like
//! there, the service changes nothing by itself: an action returns
//! [`crate::rendering::Effect`]s, the server applies them through the
//! control plane (where the room's limit and quiet hours hold, I10, K81) and
//! reports what holds with [`Volume::report`].
//!
//! `VolumeLimit` is the ceiling in the same units, so a control point draws
//! it and stops its slider there (K81). The rule above it is ohPipeline's
//! limiter (ohP `VolumeManager.cpp:229-253`): a request above the limit is
//! clamped to the limit and succeeds while the volume is below the limit,
//! and is refused with 811 when the volume already is at the limit; above
//! the top of the scale it is always 811.
//!
//! Balance and fade do not exist on a chorus target: `BalanceMax` and
//! `FadeMax` are 0 and their setters answer 801 "Action not supported" (ohP
//! `ProviderVolume.cpp:604-636`).

use super::{bool_text, parse_bool, parse_ui4, Property};
use crate::rendering::{from_thousandths, to_thousandths, Effect, MAX_VOLUME};
use crate::soap::Invocation;
use crate::{error, Outputs, UpnpError};

/// `VolumeMilliDbPerStep`: 0, meaning "no fixed decibel step". chorus's
/// volume law is not linear in decibels (a room's volume in thousandths maps
/// to gain through the server's own curve), and a made-up figure would give
/// a control point a wrong decibel readout; chorus's choice, the value only
/// feeds such a readout.
pub const MILLI_DB_PER_STEP: u32 = 0;

/// The Volume state of one renderer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Volume {
    volume: u16,
    mute: bool,
    limit: u16,
}

/// The limit on the 0 to 100 scale for a limit in thousandths: rounded
/// down, so the announced ceiling is never above the real one.
pub fn limit_of(thousandths: u16) -> u16 {
    (thousandths / 10).min(MAX_VOLUME)
}

impl Volume {
    /// The state for a target at `thousandths` volume, limited to
    /// `limit_thousandths`.
    pub fn new(thousandths: u16, mute: bool, limit_thousandths: u16) -> Volume {
        Volume {
            volume: from_thousandths(thousandths),
            mute,
            limit: limit_of(limit_thousandths),
        }
    }

    /// The volume, 0 to 100.
    pub fn volume(&self) -> u16 {
        self.volume
    }

    /// The limit, 0 to 100.
    pub fn limit(&self) -> u16 {
        self.limit
    }

    /// What the control plane holds now, whoever changed it.
    pub fn report(&mut self, thousandths: u16, mute: bool, limit_thousandths: u16) {
        self.volume = from_thousandths(thousandths);
        self.mute = mute;
        self.limit = limit_of(limit_thousandths);
    }

    /// Every evented variable with its value, in the table's order (the
    /// twelve of Volume2; ohP `ProviderVolume.cpp:247-267`).
    pub fn evented(&self) -> Vec<Property> {
        vec![
            ("Volume", self.volume.to_string()),
            ("Mute", bool_text(self.mute)),
            ("Balance", "0".to_string()),
            ("Fade", "0".to_string()),
            ("VolumeLimit", self.limit.to_string()),
            ("VolumeMax", MAX_VOLUME.to_string()),
            ("VolumeUnity", MAX_VOLUME.to_string()),
            ("VolumeSteps", MAX_VOLUME.to_string()),
            ("VolumeMilliDbPerStep", MILLI_DB_PER_STEP.to_string()),
            ("BalanceMax", "0".to_string()),
            ("FadeMax", "0".to_string()),
            ("UnityGain", bool_text(false)),
        ]
    }

    fn set(&self, wanted: u32) -> Result<Vec<Effect>, UpnpError> {
        if wanted > u32::from(MAX_VOLUME) {
            return Err(error::OH_VOLUME_INVALID);
        }
        let mut wanted = wanted as u16;
        if wanted > self.limit {
            if self.volume >= self.limit {
                return Err(error::OH_VOLUME_INVALID);
            }
            wanted = self.limit;
        }
        Ok(vec![Effect::SetVolume {
            thousandths: to_thousandths(wanted),
        }])
    }

    /// Performs a Volume action that passed [`crate::soap::validate`].
    pub fn invoke(&self, invocation: &Invocation) -> Result<(Outputs, Vec<Effect>), UpnpError> {
        let value = |v: String| Ok((vec![("Value", v)], vec![]));
        match invocation.action.name {
            "Characteristics" => Ok((
                vec![
                    ("VolumeMax", MAX_VOLUME.to_string()),
                    ("VolumeUnity", MAX_VOLUME.to_string()),
                    ("VolumeSteps", MAX_VOLUME.to_string()),
                    ("VolumeMilliDbPerStep", MILLI_DB_PER_STEP.to_string()),
                    ("BalanceMax", "0".to_string()),
                    ("FadeMax", "0".to_string()),
                ],
                vec![],
            )),
            "SetVolume" => Ok((vec![], self.set(parse_ui4(invocation.input("Value"))?)?)),
            // One step, never a fault at either end (ohP
            // `ProviderVolume.cpp:298-316`).
            "VolumeInc" => Ok((
                vec![],
                if self.volume >= self.limit {
                    vec![]
                } else {
                    self.set(u32::from(self.volume) + 1)?
                },
            )),
            "VolumeDec" => Ok((
                vec![],
                if self.volume == 0 {
                    vec![]
                } else {
                    vec![Effect::SetVolume {
                        thousandths: to_thousandths(self.volume - 1),
                    }]
                },
            )),
            "Volume" => value(self.volume.to_string()),
            "SetMute" => {
                let mute = parse_bool(invocation.input("Value"))?;
                Ok((vec![], vec![Effect::SetMute { mute }]))
            }
            "Mute" => value(bool_text(self.mute)),
            "VolumeLimit" => value(self.limit.to_string()),
            "UnityGain" => value(bool_text(false)),
            "Balance" | "Fade" => value("0".to_string()),
            "SetBalance" | "BalanceInc" | "BalanceDec" | "SetFade" | "FadeInc" | "FadeDec" => {
                Err(error::OH_VOLUME_NOT_SUPPORTED)
            }
            _ => Err(error::INVALID_ACTION),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::openhome::tables::VOLUME;

    fn call(
        v: &Volume,
        action: &str,
        inputs: &[&str],
    ) -> Result<(Outputs, Vec<Effect>), UpnpError> {
        v.invoke(&Invocation {
            action: VOLUME.action(action).unwrap(),
            inputs: inputs.iter().map(|s| s.to_string()).collect(),
        })
    }

    fn set(thousandths: u16) -> Vec<Effect> {
        vec![Effect::SetVolume { thousandths }]
    }

    #[test]
    fn the_limit_clamps_below_it_and_refuses_at_it() {
        let v = Volume::new(400, false, 600);
        assert_eq!((v.volume(), v.limit()), (40, 60));
        assert_eq!(call(&v, "VolumeLimit", &[]).unwrap().0[0].1, "60");
        assert_eq!(call(&v, "SetVolume", &["50"]).unwrap().1, set(500));
        // Above the limit from below it: clamped, a success.
        assert_eq!(call(&v, "SetVolume", &["90"]).unwrap().1, set(600));
        // At the limit: refused.
        let at = Volume::new(600, false, 600);
        assert_eq!(
            call(&at, "SetVolume", &["90"]).unwrap_err(),
            error::OH_VOLUME_INVALID
        );
        assert_eq!(call(&at, "SetVolume", &["60"]).unwrap().1, set(600));
        assert_eq!(call(&at, "SetVolume", &["10"]).unwrap().1, set(100));
        // Above the scale: always refused.
        for v in [&v, &at] {
            assert_eq!(
                call(v, "SetVolume", &["101"]).unwrap_err(),
                error::OH_VOLUME_INVALID
            );
        }
        assert_eq!(
            call(&v, "SetVolume", &["loud"]).unwrap_err(),
            error::INVALID_ARGS
        );
        // A limit that is not a whole step is announced rounded down.
        assert_eq!(Volume::new(0, false, 655).limit(), 65);
        assert_eq!(Volume::new(0, false, 1000).limit(), 100);
    }

    #[test]
    fn inc_and_dec_step_by_one_and_never_fault() {
        let v = Volume::new(400, false, 600);
        assert_eq!(call(&v, "VolumeInc", &[]).unwrap().1, set(410));
        assert_eq!(call(&v, "VolumeDec", &[]).unwrap().1, set(390));
        let at = Volume::new(600, false, 600);
        assert!(call(&at, "VolumeInc", &[]).unwrap().1.is_empty());
        let zero = Volume::new(0, false, 600);
        assert!(call(&zero, "VolumeDec", &[]).unwrap().1.is_empty());
    }

    #[test]
    fn mute_balance_fade_and_the_characteristics() {
        let mut v = Volume::new(400, false, 1000);
        assert_eq!(
            call(&v, "SetMute", &["1"]).unwrap().1,
            [Effect::SetMute { mute: true }]
        );
        v.report(250, true, 800);
        assert_eq!(call(&v, "Mute", &[]).unwrap().0[0].1, "1");
        assert_eq!(call(&v, "Volume", &[]).unwrap().0[0].1, "25");
        assert_eq!(call(&v, "VolumeLimit", &[]).unwrap().0[0].1, "80");
        for unsupported in [
            "SetBalance",
            "BalanceInc",
            "BalanceDec",
            "SetFade",
            "FadeInc",
            "FadeDec",
        ] {
            let inputs: Vec<&str> = VOLUME
                .action(unsupported)
                .unwrap()
                .inputs()
                .map(|_| "1")
                .collect();
            assert_eq!(
                call(&v, unsupported, &inputs).unwrap_err(),
                error::OH_VOLUME_NOT_SUPPORTED
            );
        }
        let (out, _) = call(&v, "Characteristics", &[]).unwrap();
        let values: Vec<&str> = out.iter().map(|(_, v)| v.as_str()).collect();
        assert_eq!(values, ["100", "100", "100", "0", "0", "0"]);
        let evented: Vec<&str> = v.evented().iter().map(|(n, _)| *n).collect();
        let table: Vec<&str> = VOLUME.variables.iter().map(|v| v.name).collect();
        assert_eq!(evented, table);
        for action in VOLUME.actions {
            let inputs: Vec<&str> = action.inputs().map(|_| "0").collect();
            if let Ok((out, _)) = call(&v, action.name, &inputs) {
                let names: Vec<&str> = out.iter().map(|(n, _)| *n).collect();
                let table: Vec<&str> = action.outputs().map(|a| a.name).collect();
                assert_eq!(names, table, "{}", action.name);
            }
        }
    }
}
