//! Firmware as the control plane shows it: the images a server has staged,
//! and what each speaker runs and is doing about an update (goal 14).
//!
//! # Nothing here installs anything
//!
//! This module holds facts and the words for them. A staged image is a file
//! the server found in its firmware directory and checked; "update available"
//! is the statement that a verified image for a speaker's board carries
//! another version than the speaker runs. Neither starts a transfer. The one
//! thing that does is the `firmware_install` command (K93, I13: an install
//! happens only on an explicit install action), and the room model's part of
//! it ([`crate::zones::Zones`]) is to say who it would reach and to refuse by
//! name what it must not.
//!
//! # Two kinds of fact, both about now
//!
//! - [`Image`]: one staged image and the verdict on it. The list is the
//!   server's last scan of its directory; it is configuration seen through a
//!   check, and it is not persisted (the directory is).
//! - [`SpeakerFirmware`]: what a speaker's own `firmware_status` said, in the
//!   catalog's words, plus the install this server process started for it, if
//!   any. It hangs on [`crate::speakers::SpeakerNow`] and is never persisted
//!   (`docs/decisions/0018-the-persisted-zone-state.md`): a server that
//!   restarts knows nothing of an install that was in progress, and resumes
//!   none.
//!
//! # A speaker's `state`
//!
//! What the speaker is doing: `idle`, `requested` (the command was accepted
//! and the offer is on its way), `receiving`, `verified` (written, digest
//! good, about to reboot into it), `pending_verify` (running the new image on
//! trial). Or how the last install this server saw ended: `confirmed`,
//! `rolled_back`, `refused`, `interrupted` (its session ended, or the server
//! found it receiving a transfer the server is not carrying), `cancelled`.
//! An outcome stays until the next install starts or the server restarts, so
//! a rollback is still on the screen when somebody looks: the speaker's own
//! later `idle` does not erase it.

use crate::json::Value;

/// The most staged images a server lists.
///
/// ASSUMED: 16. A house has a handful of board profiles and keeps a version
/// or two of each; the bound exists because every subscriber is sent the
/// whole list with every state.
pub const MAX_IMAGES: usize = 16;

/// The speaker is doing nothing about an update.
pub const IDLE: &str = "idle";
/// `firmware_install` was accepted; the offer has not been answered yet.
pub const REQUESTED: &str = "requested";
/// The speaker is writing the image it was offered.
pub const RECEIVING: &str = "receiving";
/// The image is written and its digest is good; the speaker reboots into it.
pub const VERIFIED: &str = "verified";
/// The speaker runs the new image on trial and has not confirmed it.
pub const PENDING_VERIFY: &str = "pending_verify";
/// The new image confirmed itself: the install is complete.
pub const CONFIRMED: &str = "confirmed";
/// The new image did not confirm and the bootloader went back.
pub const ROLLED_BACK: &str = "rolled_back";
/// The speaker refused the offer or the image.
pub const REFUSED: &str = "refused";
/// The install ended with its session, or was found orphaned and abandoned.
pub const INTERRUPTED: &str = "interrupted";
/// The owner cancelled the install (`firmware_cancel`).
pub const CANCELLED: &str = "cancelled";

/// The reason that is no reason.
pub const NO_REASON: &str = "none";

/// One staged image and the server's verdict on it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    /// Its name: the file stem of `<name>.bin` and `<name>.manifest`, which
    /// is what `firmware_install` names.
    pub name: String,
    /// The version its manifest declares; empty where the manifest could not
    /// be read.
    pub version: String,
    /// The board profile its manifest declares it was built for.
    pub board: String,
    /// The size its manifest declares, bytes.
    pub size: u64,
    /// The SHA-256 its manifest declares, 64 lower-case hex digits; empty
    /// where the manifest could not be read.
    pub sha256: String,
    /// Why it is refused, by name; `None` when it is verified. A refused
    /// image is listed and never offered.
    pub refused: Option<String>,
}

impl Image {
    /// Whether the server verified it: only such an image is ever offered.
    pub fn verified(&self) -> bool {
        self.refused.is_none()
    }
}

/// One image as the state message's `firmware.images` array holds it, in the
/// declared field order. `reason` is written only for a refused image.
pub fn image_value(image: &Image) -> Value {
    let mut fields = vec![
        ("name".to_string(), Value::text(&image.name)),
        ("version".to_string(), Value::text(&image.version)),
        ("board".to_string(), Value::text(&image.board)),
        ("size".to_string(), Value::int(image.size as i64)),
        ("sha256".to_string(), Value::text(&image.sha256)),
        (
            "verdict".to_string(),
            Value::text(if image.verified() {
                "verified"
            } else {
                "refused"
            }),
        ),
    ];
    if let Some(reason) = &image.refused {
        fields.push(("reason".to_string(), Value::text(reason)));
    }
    Value::Obj(fields)
}

/// The install a speaker was last given by this server process, or the one
/// its own status named.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Install {
    /// The staged image's name; empty where this server did not start the
    /// install (it restarted since) and only the speaker's word is known.
    pub image: String,
    /// The version being installed.
    pub version: String,
    /// The transfer id the offer carries; 0 until the server assigned one.
    pub transfer: u32,
    /// Bytes the speaker has written in order, by its last acknowledgement.
    pub received: u64,
    /// The image's size, bytes; 0 where it is not known.
    pub size: u64,
}

/// What a speaker's `firmware_status` said, in the catalog's words: the
/// session layer translates the wire's enums to these names and this crate
/// reads no wire.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    /// The state's wire name (`idle`, `receiving`, `verified`,
    /// `pending_verify`, `confirmed`, `rolled_back`, `refused`).
    pub state: String,
    /// The reason's wire name (`none`, `bad_digest`, ...).
    pub reason: String,
    /// The transfer the status is about; 0 when none.
    pub transfer: u32,
    /// Bytes written in order so far.
    pub received: u64,
    /// The version of the image the speaker RUNS.
    pub version: String,
    /// The board profile the speaker claims.
    pub board: String,
    /// The slot it runs from, where it knows.
    pub slot: Option<u8>,
    /// The version of the image the status is about, where it names one (a
    /// rollback names the image that was tried).
    pub image_version: String,
    /// Whether the server is carrying the transfer the status names, on the
    /// session it arrived on.
    pub carried: bool,
}

/// What is known of a speaker's firmware now. Never persisted.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SpeakerFirmware {
    /// The version it runs.
    pub version: String,
    /// The board profile it claims: an image for another board is not for it.
    pub board: String,
    /// The slot it runs from, where it said.
    pub slot: Option<u8>,
    /// What it is doing, or how its last install ended (the module's docs).
    pub state: String,
    /// Why, where the state has a reason; [`NO_REASON`] otherwise.
    pub reason: String,
    /// The install the state is about, if any.
    pub install: Option<Install>,
}

impl SpeakerFirmware {
    /// Whether an install is in progress: a second one is refused until this
    /// one ends.
    pub fn busy(&self) -> bool {
        matches!(
            self.state.as_str(),
            REQUESTED | RECEIVING | VERIFIED | PENDING_VERIFY
        )
    }

    /// Whether the state is an outcome: it stays until the next install.
    fn settled(&self) -> bool {
        matches!(
            self.state.as_str(),
            CONFIRMED | ROLLED_BACK | REFUSED | INTERRUPTED | CANCELLED
        )
    }

    /// The command was accepted: the speaker is to be offered `image`.
    pub fn requested(&mut self, image: &Image) {
        self.state = REQUESTED.to_string();
        self.reason = NO_REASON.to_string();
        self.install = Some(Install {
            image: image.name.clone(),
            version: image.version.clone(),
            transfer: 0,
            received: 0,
            size: image.size,
        });
    }

    /// The install in progress ended without the speaker saying how: its
    /// session went away, or the owner cancelled it. Nothing changes unless a
    /// transfer was on its way (`requested` or `receiving`): an image already
    /// verified reboots the speaker by itself and is not interrupted by the
    /// session that ends with it.
    pub fn ended(&mut self, outcome: &str, reason: &str) -> bool {
        if !matches!(self.state.as_str(), REQUESTED | RECEIVING) {
            return false;
        }
        self.state = outcome.to_string();
        self.reason = reason.to_string();
        true
    }

    /// Take in what the speaker said. `current` is `None` until its first
    /// status; a speaker that never sends one takes no updates.
    pub fn absorb(current: &mut Option<SpeakerFirmware>, report: &Report) {
        let fw = current.get_or_insert_with(|| SpeakerFirmware {
            state: IDLE.to_string(),
            reason: NO_REASON.to_string(),
            ..SpeakerFirmware::default()
        });
        fw.version = report.version.clone();
        fw.board = report.board.clone();
        fw.slot = report.slot;
        // Whether the status is about the install this record already holds.
        let same = report.transfer != 0
            && fw
                .install
                .as_ref()
                .is_some_and(|i| i.transfer == report.transfer);
        // The install a status names when this server did not start it (it
        // restarted since): only what the speaker itself said is known.
        let named = |version: &str| Install {
            image: String::new(),
            version: version.to_string(),
            transfer: report.transfer,
            received: 0,
            size: 0,
        };
        match report.state.as_str() {
            IDLE => {
                if !fw.settled() {
                    fw.state = IDLE.to_string();
                    fw.reason = NO_REASON.to_string();
                    fw.install = None;
                }
            }
            RECEIVING => {
                if report.carried {
                    fw.state = RECEIVING.to_string();
                    fw.reason = NO_REASON.to_string();
                    if let Some(install) = &mut fw.install {
                        install.received = report.received;
                    }
                } else {
                    // A transfer nobody here is sending: the server restarted,
                    // or the session it travelled in ended. It is not resumed
                    // (the session layer tells the speaker to abandon it).
                    if !same {
                        fw.install = Some(named(&report.image_version));
                    }
                    fw.state = INTERRUPTED.to_string();
                    fw.reason = "not_resumed".to_string();
                }
            }
            VERIFIED => {
                fw.state = VERIFIED.to_string();
                fw.reason = NO_REASON.to_string();
                if !same {
                    fw.install = Some(named(&report.image_version));
                }
                if let Some(install) = &mut fw.install {
                    install.received = report.received;
                }
            }
            PENDING_VERIFY | CONFIRMED => {
                // The image on trial, or just confirmed, is the one running.
                fw.state = report.state.clone();
                fw.reason = report.reason.clone();
                if !same {
                    fw.install = Some(named(&report.version));
                }
            }
            ROLLED_BACK => {
                fw.state = ROLLED_BACK.to_string();
                fw.reason = report.reason.clone();
                if !same {
                    fw.install = Some(named(&report.image_version));
                }
            }
            // A refusal of an offer that is not this record's install
            // changes nothing: the install it holds is not the one refused.
            REFUSED if same || report.carried => {
                fw.state = REFUSED.to_string();
                fw.reason = report.reason.clone();
                if let Some(install) = &mut fw.install {
                    install.received = report.received;
                }
            }
            _ => {}
        }
    }
}

/// Whether a staged, verified image for the speaker's board carries another
/// version than it runs. Information only: nothing is installed because of it.
pub fn update_available(firmware: &SpeakerFirmware, images: &[Image]) -> bool {
    images
        .iter()
        .any(|i| i.verified() && i.board == firmware.board && i.version != firmware.version)
}

/// A speaker's `firmware` object in the state message, in the declared field
/// order, every member always written: `image` is `null` and the progress is
/// zero where there is no install to speak of.
pub fn speaker_firmware_value(firmware: &SpeakerFirmware, images: &[Image]) -> Value {
    let install = firmware.install.clone().unwrap_or_default();
    Value::Obj(vec![
        ("version".to_string(), Value::text(&firmware.version)),
        ("board".to_string(), Value::text(&firmware.board)),
        (
            "slot".to_string(),
            match firmware.slot {
                Some(slot) => Value::int(i64::from(slot)),
                None => Value::Null,
            },
        ),
        ("state".to_string(), Value::text(&firmware.state)),
        ("reason".to_string(), Value::text(&firmware.reason)),
        (
            "update_available".to_string(),
            Value::Bool(update_available(firmware, images)),
        ),
        (
            "image".to_string(),
            if install.image.is_empty() {
                Value::Null
            } else {
                Value::text(&install.image)
            },
        ),
        ("image_version".to_string(), Value::text(&install.version)),
        ("received".to_string(), Value::int(install.received as i64)),
        ("size".to_string(), Value::int(install.size as i64)),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(name: &str, version: &str, board: &str) -> Image {
        Image {
            name: name.to_string(),
            version: version.to_string(),
            board: board.to_string(),
            size: 4096,
            sha256: "00".repeat(32),
            refused: None,
        }
    }

    fn report(state: &str, transfer: u32, version: &str) -> Report {
        Report {
            state: state.to_string(),
            reason: NO_REASON.to_string(),
            transfer,
            received: 0,
            version: version.to_string(),
            board: "brick".to_string(),
            slot: Some(0),
            image_version: String::new(),
            carried: false,
        }
    }

    #[test]
    fn an_update_is_available_only_from_a_verified_image_for_the_same_board_and_another_version() {
        let mut fw = None;
        SpeakerFirmware::absorb(&mut fw, &report(IDLE, 0, "1.0.0"));
        let fw = fw.unwrap();
        assert!(!update_available(&fw, &[]));
        assert!(!update_available(&fw, &[image("a", "1.0.0", "brick")]));
        assert!(!update_available(&fw, &[image("a", "2.0.0", "compact")]));
        let mut refused = image("a", "2.0.0", "brick");
        refused.refused = Some("digest-mismatch".to_string());
        assert!(!update_available(&fw, &[refused]));
        assert!(update_available(&fw, &[image("a", "2.0.0", "brick")]));
        // Another version, not a newer one: versions are names, not numbers,
        // and going back is an install like any other.
        assert!(update_available(&fw, &[image("a", "0.9.0", "brick")]));
    }

    #[test]
    fn an_install_runs_from_requested_to_confirmed_and_the_outcome_stays() {
        let mut fw = None;
        SpeakerFirmware::absorb(&mut fw, &report(IDLE, 0, "1.0.0"));
        let staged = image("good", "2.0.0", "brick");
        fw.as_mut().unwrap().requested(&staged);
        assert!(fw.as_ref().unwrap().busy());
        fw.as_mut().unwrap().install.as_mut().unwrap().transfer = 7;

        let mut receiving = report(RECEIVING, 7, "1.0.0");
        receiving.received = 2048;
        receiving.carried = true;
        SpeakerFirmware::absorb(&mut fw, &receiving);
        let now = fw.clone().unwrap();
        assert_eq!(now.state, RECEIVING);
        assert_eq!(now.install.as_ref().unwrap().received, 2048);
        assert_eq!(now.install.as_ref().unwrap().image, "good");

        let mut verified = report(VERIFIED, 7, "1.0.0");
        verified.received = 4096;
        SpeakerFirmware::absorb(&mut fw, &verified);
        assert_eq!(fw.as_ref().unwrap().state, VERIFIED);
        // The session ends as the speaker reboots: not an interruption.
        assert!(!fw.as_mut().unwrap().ended(INTERRUPTED, "session_ended"));

        SpeakerFirmware::absorb(&mut fw, &report(PENDING_VERIFY, 7, "2.0.0"));
        assert_eq!(fw.as_ref().unwrap().state, PENDING_VERIFY);
        assert!(fw.as_ref().unwrap().busy());
        SpeakerFirmware::absorb(&mut fw, &report(CONFIRMED, 7, "2.0.0"));
        let now = fw.clone().unwrap();
        assert_eq!(
            (now.state.as_str(), now.version.as_str()),
            (CONFIRMED, "2.0.0")
        );
        assert_eq!(now.install.as_ref().unwrap().image, "good");
        assert!(!now.busy());
        // Later sessions say idle; the outcome is still what the state shows.
        SpeakerFirmware::absorb(&mut fw, &report(IDLE, 0, "2.0.0"));
        assert_eq!(fw.as_ref().unwrap().state, CONFIRMED);
        assert!(!update_available(fw.as_ref().unwrap(), &[staged]));
    }

    #[test]
    fn a_rollback_names_the_image_tried_and_what_runs_again() {
        let mut fw = None;
        let mut rolled = report(ROLLED_BACK, 9, "2.0.0");
        rolled.reason = "not_confirmed".to_string();
        rolled.image_version = "3.0.0".to_string();
        SpeakerFirmware::absorb(&mut fw, &rolled);
        let now = fw.clone().unwrap();
        assert_eq!(now.state, ROLLED_BACK);
        assert_eq!(now.reason, "not_confirmed");
        assert_eq!(now.version, "2.0.0");
        let install = now.install.unwrap();
        assert_eq!(
            (install.image.as_str(), install.version.as_str()),
            ("", "3.0.0")
        );
        SpeakerFirmware::absorb(&mut fw, &report(IDLE, 0, "2.0.0"));
        assert_eq!(fw.unwrap().state, ROLLED_BACK, "an outcome stays");
    }

    #[test]
    fn a_transfer_nobody_is_carrying_is_interrupted_and_never_shown_as_receiving() {
        // A server that restarted finds the speaker mid-download.
        let mut fw = None;
        let mut orphan = report(RECEIVING, 31, "1.0.0");
        orphan.received = 8192;
        SpeakerFirmware::absorb(&mut fw, &orphan);
        let now = fw.clone().unwrap();
        assert_eq!(
            (now.state.as_str(), now.reason.as_str()),
            (INTERRUPTED, "not_resumed")
        );
        assert!(!now.busy());
        // The speaker abandons it and says idle; the outcome stays.
        SpeakerFirmware::absorb(&mut fw, &report(IDLE, 0, "1.0.0"));
        assert_eq!(fw.unwrap().state, INTERRUPTED);
    }

    #[test]
    fn a_session_that_ends_mid_transfer_interrupts_it_and_a_cancel_cancels_it() {
        let mut fw = None;
        SpeakerFirmware::absorb(&mut fw, &report(IDLE, 0, "1.0.0"));
        let mut fw = fw.unwrap();
        assert!(!fw.ended(CANCELLED, NO_REASON), "nothing to cancel");
        fw.requested(&image("good", "2.0.0", "brick"));
        assert!(fw.ended(INTERRUPTED, "session_ended"));
        assert_eq!(fw.state, INTERRUPTED);
        fw.requested(&image("good", "2.0.0", "brick"));
        assert!(fw.ended(CANCELLED, NO_REASON));
        assert_eq!(fw.state, CANCELLED);
    }

    #[test]
    fn a_refusal_of_another_offer_does_not_touch_the_install_held() {
        let mut fw = None;
        SpeakerFirmware::absorb(&mut fw, &report(IDLE, 0, "1.0.0"));
        fw.as_mut()
            .unwrap()
            .requested(&image("good", "2.0.0", "brick"));
        fw.as_mut().unwrap().install.as_mut().unwrap().transfer = 7;
        let mut other = report(REFUSED, 99, "1.0.0");
        other.reason = "busy".to_string();
        SpeakerFirmware::absorb(&mut fw, &other);
        assert_eq!(fw.as_ref().unwrap().state, REQUESTED);
        let mut ours = report(REFUSED, 7, "1.0.0");
        ours.reason = "wrong_board".to_string();
        SpeakerFirmware::absorb(&mut fw, &ours);
        let now = fw.unwrap();
        assert_eq!(
            (now.state.as_str(), now.reason.as_str()),
            (REFUSED, "wrong_board")
        );
    }

    #[test]
    fn the_state_objects_have_one_shape() {
        let mut refused = image("bad", "2.0.0", "brick");
        refused.refused = Some("digest-mismatch".to_string());
        assert_eq!(
            crate::json::write(&image_value(&refused)),
            format!(
                r#"{{"name":"bad","version":"2.0.0","board":"brick","size":4096,"sha256":"{}","verdict":"refused","reason":"digest-mismatch"}}"#,
                "00".repeat(32)
            )
        );
        let mut fw = None;
        SpeakerFirmware::absorb(&mut fw, &report(IDLE, 0, "1.0.0"));
        assert_eq!(
            crate::json::write(&speaker_firmware_value(
                fw.as_ref().unwrap(),
                &[image("good", "2.0.0", "brick")]
            )),
            r#"{"version":"1.0.0","board":"brick","slot":0,"state":"idle","reason":"none","update_available":true,"image":null,"image_version":"","received":0,"size":0}"#
        );
    }
}
