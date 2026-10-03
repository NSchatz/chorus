//! The speakers this server has adopted: what each is called, which room it
//! was assigned, and what is known about it right now (goal 14).
//!
//! # What a speaker record is, and what it is not
//!
//! Adoption itself is a pinned key (`docs/protocol.md`, "Adoption: trust on
//! first use"; the server keeps the pins in `adopted-endpoints`, and nothing
//! here reads or writes that file). A pin says "this id is this key" and
//! nothing a person can use: no name, no room, no way to see that a speaker
//! arrived. A [`Speaker`] is the record beside the pin, one per adopted id,
//! created by the server the moment the id is adopted and named and assigned
//! afterwards through the catalog (`speaker_name`, `speaker_room`,
//! `speaker_forget`; K92: auto-adopt on the LAN, name in the app).
//!
//! # Two halves, kept apart on purpose
//!
//! - What a PERSON set is persisted (state-file format 5): the name, whether
//!   it was ever named, and the room it was assigned.
//! - What is true NOW is [`SpeakerNow`], never persisted
//!   (`docs/decisions/0018-the-persisted-zone-state.md`): whether a session
//!   of it is up, the software its `hello` named, its roles and its key's
//!   fingerprint. The session layer says these; nothing here reads a clock or
//!   a socket.
//!
//! [`SpeakerNow`] is the one place a per-speaker runtime fact hangs. A later
//! fact (a firmware version, an update's progress) is a field there, set
//! through [`Speakers::set_now`], and written into the speaker's object in
//! the state message by [`speaker_value`].
//!
//! # Key changes
//!
//! A session under an adopted id with another key is refused and never
//! re-pinned (`an_endpoint_whose_key_changed_is_refused_and_surfaced`,
//! `crates/protocol/tests/v2_vectors.rs`). [`KeyChange`] is that refusal as
//! the control plane shows it, one per id (the latest), until the owner
//! forgets the speaker (`speaker_forget`, the only way past a changed key) or
//! the server restarts.

use crate::catalog::{is_display_name, is_identifier};
use crate::firmware::{speaker_firmware_value, Image, SpeakerFirmware};
use crate::json::Value;
use crate::rooms::Link;

/// The most speaker records a server holds.
///
/// ASSUMED: 64, twice the bound on every other definition the catalog holds
/// (`crate::rooms::MAX_DEFINITIONS`), and far above a house's speaker count.
/// It exists because adoption is automatic: any peer on the LAN can present a
/// new id, and the state message every subscriber is sent must not grow with
/// each one. An id adopted past the bound still has its pin and still plays;
/// it is not listed until a record is forgotten.
pub const MAX_SPEAKERS: usize = 64;

/// What is known about a speaker right now. Never persisted.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SpeakerNow {
    /// How many sessions under this id are up. More than one is possible for
    /// a moment while a speaker reconnects.
    pub sessions: u32,
    /// The software string of its latest `hello`; empty until one arrived.
    pub software: String,
    /// The role names of its latest `hello`, in the protocol's order; empty
    /// until one arrived.
    pub roles: Vec<String>,
    /// The fingerprint of the key it is pinned to; empty where the server
    /// has not said (a record loaded from a state file whose pin is gone).
    pub key: String,
    /// (goal 14, explicit installs) What it runs and what it is doing about
    /// an update, from its own `firmware_status`; `None` until it has sent
    /// one (a speaker that never does takes no updates).
    pub firmware: Option<SpeakerFirmware>,
}

impl SpeakerNow {
    /// Whether a session of this speaker is up.
    pub fn present(&self) -> bool {
        self.sessions > 0
    }
}

/// One adopted speaker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Speaker {
    /// The id its sessions authenticate as, which is what commands name.
    pub id: String,
    /// What a person sees: [`default_name`] until somebody names it.
    pub name: String,
    /// Whether a person has named it. An app lists the ones that are not.
    pub named: bool,
    /// The room it was assigned with `speaker_room`, or `None`.
    pub room: Option<String>,
    /// What is true of it now.
    pub now: SpeakerNow,
}

impl Speaker {
    /// A freshly adopted speaker: unnamed, in no room, nothing known.
    pub fn new(id: &str) -> Speaker {
        Speaker {
            id: id.to_string(),
            name: default_name(id),
            named: false,
            room: None,
            now: SpeakerNow::default(),
        }
    }
}

/// The name a speaker has until it is named: `Speaker ` and the last four
/// characters of its id (the whole id where it is shorter).
pub fn default_name(id: &str) -> String {
    let tail: String = {
        let chars: Vec<char> = id.chars().collect();
        chars[chars.len().saturating_sub(4)..].iter().collect()
    };
    format!("Speaker {}", tail)
}

/// A refused handshake under an adopted id: the key it is pinned to and the
/// one that was offered, as fingerprints.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyChange {
    /// The id the handshake claimed.
    pub id: String,
    /// The fingerprint of the pinned key, which did not move.
    pub pinned: String,
    /// The fingerprint of the key that was offered and refused.
    pub offered: String,
}

/// Why a speaker could not be recorded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotListed {
    /// Its id is not a catalog identifier, so no command could name it.
    NotAnIdentifier,
    /// The server already holds [`MAX_SPEAKERS`] records.
    Full,
}

impl NotListed {
    /// The reason's name, for a log line.
    pub fn name(&self) -> &'static str {
        match self {
            NotListed::NotAnIdentifier => "id-not-an-identifier",
            NotListed::Full => "registry-full",
        }
    }
}

/// Every speaker record, sorted by id, and the key changes refused since the
/// server started.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Speakers {
    speakers: Vec<Speaker>,
    key_changes: Vec<KeyChange>,
}

impl Speakers {
    /// Every record, sorted by id.
    pub fn all(&self) -> &[Speaker] {
        &self.speakers
    }

    /// One record by id.
    pub fn get(&self, id: &str) -> Option<&Speaker> {
        self.speakers.iter().find(|s| s.id == id)
    }

    pub(crate) fn get_mut(&mut self, id: &str) -> Option<&mut Speaker> {
        self.speakers.iter_mut().find(|s| s.id == id)
    }

    /// Whether there is nothing to show: no record and no key change.
    pub fn is_empty(&self) -> bool {
        self.speakers.is_empty() && self.key_changes.is_empty()
    }

    /// Every key change refused, sorted by id, the latest per id.
    pub fn key_changes(&self) -> &[KeyChange] {
        &self.key_changes
    }

    /// The record for `id`, created unnamed if there is none. `Ok(true)`
    /// when it was created.
    pub fn adopt(&mut self, id: &str) -> Result<bool, NotListed> {
        if self.get(id).is_some() {
            return Ok(false);
        }
        if !is_identifier(id) {
            return Err(NotListed::NotAnIdentifier);
        }
        if self.speakers.len() >= MAX_SPEAKERS {
            return Err(NotListed::Full);
        }
        self.speakers.push(Speaker::new(id));
        self.speakers.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(true)
    }

    /// Install a persisted record, which loading persisted state does. The
    /// caller has validated it.
    pub fn restore(&mut self, speaker: Speaker) {
        self.speakers.retain(|s| s.id != speaker.id);
        self.speakers.push(speaker);
        self.speakers.sort_by(|a, b| a.id.cmp(&b.id));
    }

    /// Change what is known about a speaker now. Whether anything changed;
    /// `false` too where there is no such record.
    pub fn set_now(&mut self, id: &str, change: impl FnOnce(&mut SpeakerNow)) -> bool {
        let Some(speaker) = self.get_mut(id) else {
            return false;
        };
        let before = speaker.now.clone();
        change(&mut speaker.now);
        speaker.now != before
    }

    /// Remove a record and any key change under its id. Whether there was
    /// either.
    pub(crate) fn forget(&mut self, id: &str) -> bool {
        let before = (self.speakers.len(), self.key_changes.len());
        self.speakers.retain(|s| s.id != id);
        self.key_changes.retain(|c| c.id != id);
        before != (self.speakers.len(), self.key_changes.len())
    }

    /// Record a refused key change, replacing an earlier one under the same
    /// id. Whether anything changed.
    pub(crate) fn key_changed(&mut self, change: KeyChange) -> bool {
        if self.key_changes.contains(&change) {
            return false;
        }
        self.key_changes.retain(|c| c.id != change.id);
        self.key_changes.push(change);
        self.key_changes.sort_by(|a, b| a.id.cmp(&b.id));
        true
    }

    /// The ids, for a refusal that names what there is.
    pub(crate) fn id_list(&self) -> String {
        if self.speakers.is_empty() {
            return "none".to_string();
        }
        self.speakers
            .iter()
            .map(|s| s.id.clone())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// Whether `name` is a name a speaker may be given: the catalog's display
/// name rule, the one a room's name is held to.
pub fn is_speaker_name(name: &str) -> bool {
    is_display_name(name)
}

/// One speaker as the state message's `speakers` array holds it, in the
/// declared field order. `link` is what the endpoint reported with `attach`
/// (the `endpoints` array's fact), `unknown` where it never has.
///
/// A per-speaker fact added later is appended here, after `roles`, so the
/// members before it keep their order. The first is `firmware` (goal 14),
/// written only once the speaker has said what it runs, so a speaker that
/// takes no updates keeps the bytes it had; `update_available` in it is
/// derived here from the staged `images`, never stored.
pub fn speaker_value(speaker: &Speaker, link: Link, images: &[Image]) -> Value {
    let mut fields = vec![
        ("id".to_string(), Value::text(&speaker.id)),
        ("name".to_string(), Value::text(&speaker.name)),
        ("named".to_string(), Value::Bool(speaker.named)),
        (
            "room".to_string(),
            match &speaker.room {
                Some(room) => Value::text(room),
                None => Value::Null,
            },
        ),
        ("present".to_string(), Value::Bool(speaker.now.present())),
        ("software".to_string(), Value::text(&speaker.now.software)),
        ("link".to_string(), Value::text(link.name())),
        ("key".to_string(), Value::text(&speaker.now.key)),
        (
            "roles".to_string(),
            Value::Arr(speaker.now.roles.iter().map(|r| Value::text(r)).collect()),
        ),
    ];
    if let Some(firmware) = &speaker.now.firmware {
        fields.push((
            "firmware".to_string(),
            speaker_firmware_value(firmware, images),
        ));
    }
    Value::Obj(fields)
}

/// One key change as the state message's `key_changes` array holds it.
pub fn key_change_value(change: &KeyChange) -> Value {
    Value::Obj(vec![
        ("id".to_string(), Value::text(&change.id)),
        ("pinned".to_string(), Value::text(&change.pinned)),
        ("offered".to_string(), Value::text(&change.offered)),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_speaker_is_called_by_the_tail_of_its_id_until_it_is_named() {
        assert_eq!(default_name("chorus-0123456789ab"), "Speaker 89ab");
        assert_eq!(default_name("ab"), "Speaker ab");
        let s = Speaker::new("chorus-0123456789ab");
        assert!(!s.named);
        assert_eq!(s.room, None);
        assert!(!s.now.present());
        assert!(is_speaker_name(&s.name));
    }

    #[test]
    fn adoption_makes_one_record_per_id_and_refuses_by_name_what_it_cannot_list() {
        let mut speakers = Speakers::default();
        assert_eq!(speakers.adopt("den"), Ok(true));
        assert_eq!(speakers.adopt("den"), Ok(false), "known already");
        assert_eq!(
            speakers.adopt("Not An Identifier"),
            Err(NotListed::NotAnIdentifier)
        );
        for n in 0..MAX_SPEAKERS - 1 {
            assert_eq!(speakers.adopt(&format!("s-{}", n)), Ok(true));
        }
        assert_eq!(speakers.adopt("one-too-many"), Err(NotListed::Full));
        assert_eq!(NotListed::Full.name(), "registry-full");
        assert!(speakers.forget("den"));
        assert_eq!(speakers.adopt("one-too-many"), Ok(true));
        let ids: Vec<&str> = speakers.all().iter().map(|s| s.id.as_str()).collect();
        let mut sorted = ids.clone();
        sorted.sort();
        assert_eq!(ids, sorted, "the records are kept sorted by id");
    }

    #[test]
    fn a_key_change_is_kept_once_per_id_and_goes_with_the_speaker() {
        let mut speakers = Speakers::default();
        let change = |offered: &str| KeyChange {
            id: "den".to_string(),
            pinned: "aaaa".to_string(),
            offered: offered.to_string(),
        };
        assert!(speakers.key_changed(change("bbbb")));
        assert!(!speakers.key_changed(change("bbbb")), "the same again");
        assert!(speakers.key_changed(change("cccc")));
        assert_eq!(speakers.key_changes(), &[change("cccc")]);
        assert!(!speakers.is_empty());
        assert!(speakers.forget("den"));
        assert!(speakers.is_empty());
    }
}
