//! Adoption: trust on first use, with the key pinned (decision K92).
//!
//! The first time a peer id completes a handshake, its long-term public key is
//! pinned to that id. Every later handshake under the same id must present the
//! same key; a different one is refused and surfaced, never silently
//! re-pinned. Only the owner changes a pin (by forgetting the endpoint, which
//! lets it be adopted afresh), and a removed endpoint is refused rather than
//! re-adopted until the owner forgets it.
//!
//! The same store serves both sides: the server pins endpoints, and an
//! endpoint pins its server. It is plain data with a line-oriented text form
//! so a server can persist it next to its other state; the file is written by
//! the caller, never by this module.

use std::collections::BTreeMap;
use std::fmt;

use crate::v2::noise::{fingerprint, KEY_LEN};

/// Longest peer id: the one-byte length of the handshake payload.
pub const MAX_ID_LEN: usize = 255;

/// What the store says about a handshake.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// The id was new: its key is now pinned. Proceed.
    Adopted,
    /// The id and key match the pin. Proceed.
    Known,
    /// The id is pinned to another key. Refuse and surface.
    KeyChanged {
        /// The key pinned at adoption.
        pinned: [u8; KEY_LEN],
        /// The key the peer presented.
        offered: [u8; KEY_LEN],
    },
    /// The owner removed this id. Refuse.
    Removed,
}

impl Verdict {
    /// Whether the session may proceed.
    pub fn admits(&self) -> bool {
        matches!(self, Verdict::Adopted | Verdict::Known)
    }
}

/// A key change, as surfaced to people: who, which pin, which offer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyChange {
    /// The peer id.
    pub id: String,
    /// Fingerprint of the pinned key.
    pub pinned: String,
    /// Fingerprint of the key presented.
    pub offered: String,
}

impl fmt::Display for KeyChange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} presented key {} but key {} has been pinned since its adoption; refused",
            self.id, self.offered, self.pinned
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Entry {
    Pinned([u8; KEY_LEN]),
    Removed([u8; KEY_LEN]),
}

/// Why a store's text could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    /// 1-based line.
    pub line: usize,
    /// What is wrong.
    pub reason: &'static str,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.reason)
    }
}

impl std::error::Error for ParseError {}

/// Pinned keys by peer id.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PinStore {
    entries: BTreeMap<String, Entry>,
    key_changes: Vec<KeyChange>,
}

impl PinStore {
    /// An empty store: the first peer of every id is adopted.
    pub fn new() -> PinStore {
        PinStore::default()
    }

    /// Check a handshake's id and key, pinning the key when the id is new.
    ///
    /// A key change is also recorded in [`PinStore::key_changes`], so a caller
    /// that only polls the store still sees it.
    pub fn check(&mut self, id: &str, key: &[u8; KEY_LEN]) -> Verdict {
        match self.entries.get(id) {
            None => {
                self.entries.insert(id.to_string(), Entry::Pinned(*key));
                Verdict::Adopted
            }
            Some(Entry::Pinned(pinned)) if pinned == key => Verdict::Known,
            Some(Entry::Pinned(pinned)) => {
                let pinned = *pinned;
                self.key_changes.push(KeyChange {
                    id: id.to_string(),
                    pinned: fingerprint(&pinned),
                    offered: fingerprint(key),
                });
                Verdict::KeyChanged {
                    pinned,
                    offered: *key,
                }
            }
            Some(Entry::Removed(_)) => Verdict::Removed,
        }
    }

    /// The owner's act: stop admitting this id until it is forgotten.
    pub fn remove(&mut self, id: &str) -> bool {
        match self.entries.get(id).cloned() {
            Some(Entry::Pinned(k)) => {
                self.entries.insert(id.to_string(), Entry::Removed(k));
                true
            }
            _ => false,
        }
    }

    /// The owner's act: forget this id entirely, so it is adopted afresh on its
    /// next handshake. This is the only way a changed key is ever accepted.
    pub fn forget(&mut self, id: &str) -> bool {
        self.key_changes.retain(|c| c.id != id);
        self.entries.remove(id).is_some()
    }

    /// The pinned key for an id, if it is pinned (not removed).
    pub fn pinned(&self, id: &str) -> Option<[u8; KEY_LEN]> {
        match self.entries.get(id) {
            Some(Entry::Pinned(k)) => Some(*k),
            _ => None,
        }
    }

    /// Every key change refused since the store was loaded.
    pub fn key_changes(&self) -> &[KeyChange] {
        &self.key_changes
    }

    /// Ids with a pin, in order.
    pub fn ids(&self) -> Vec<String> {
        self.entries.keys().cloned().collect()
    }

    /// The text form: one `<state> <key hex> <id>` line per id, `#` comments.
    pub fn to_text(&self) -> String {
        let mut out =
            String::from("# chorus adopted peers: <pinned|removed> <public key hex> <id>\n");
        for (id, entry) in &self.entries {
            let (state, key) = match entry {
                Entry::Pinned(k) => ("pinned", k),
                Entry::Removed(k) => ("removed", k),
            };
            out.push_str(state);
            out.push(' ');
            for b in key {
                out.push_str(&format!("{:02x}", b));
            }
            out.push(' ');
            out.push_str(id);
            out.push('\n');
        }
        out
    }

    /// Read the text form back.
    pub fn from_text(text: &str) -> Result<PinStore, ParseError> {
        let mut store = PinStore::new();
        for (i, raw) in text.lines().enumerate() {
            let line = raw.trim_end();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let bad = |reason| ParseError {
                line: i + 1,
                reason,
            };
            let mut parts = line.splitn(3, ' ');
            let state = parts.next().ok_or(bad("no state"))?;
            let key_hex = parts.next().ok_or(bad("no key"))?;
            let id = parts.next().ok_or(bad("no id"))?;
            if id.is_empty() || id.len() > MAX_ID_LEN {
                return Err(bad("an id is 1 to 255 bytes"));
            }
            if key_hex.len() != KEY_LEN * 2 {
                return Err(bad("a key is 64 hex digits"));
            }
            let mut key = [0u8; KEY_LEN];
            for (j, k) in key.iter_mut().enumerate() {
                *k = u8::from_str_radix(&key_hex[j * 2..j * 2 + 2], 16)
                    .map_err(|_| bad("a key is hex"))?;
            }
            let entry = match state {
                "pinned" => Entry::Pinned(key),
                "removed" => Entry::Removed(key),
                _ => return Err(bad("the state is pinned or removed")),
            };
            if store.entries.insert(id.to_string(), entry).is_some() {
                return Err(bad("an id appears twice"));
            }
        }
        Ok(store)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_use_pins_and_the_same_key_is_known() {
        let mut s = PinStore::new();
        assert_eq!(s.check("kitchen", &[1; 32]), Verdict::Adopted);
        assert_eq!(s.check("kitchen", &[1; 32]), Verdict::Known);
        assert_eq!(s.pinned("kitchen"), Some([1; 32]));
    }

    #[test]
    fn a_changed_key_is_refused_recorded_and_not_repinned() {
        let mut s = PinStore::new();
        s.check("kitchen", &[1; 32]);
        let v = s.check("kitchen", &[2; 32]);
        assert_eq!(
            v,
            Verdict::KeyChanged {
                pinned: [1; 32],
                offered: [2; 32]
            }
        );
        assert!(!v.admits());
        assert_eq!(s.pinned("kitchen"), Some([1; 32]));
        assert_eq!(s.key_changes().len(), 1);
        assert_eq!(s.key_changes()[0].id, "kitchen");
        // Asking again does not wear the pin down.
        assert!(!s.check("kitchen", &[2; 32]).admits());
        // Only forgetting lets the new key in.
        assert!(s.forget("kitchen"));
        assert_eq!(s.check("kitchen", &[2; 32]), Verdict::Adopted);
    }

    #[test]
    fn a_removed_peer_is_refused_even_with_its_old_key() {
        let mut s = PinStore::new();
        s.check("den", &[3; 32]);
        assert!(s.remove("den"));
        assert_eq!(s.check("den", &[3; 32]), Verdict::Removed);
    }

    #[test]
    fn the_text_form_round_trips() {
        let mut s = PinStore::new();
        s.check("den", &[3; 32]);
        s.check("living room", &[4; 32]);
        s.remove("den");
        let back = PinStore::from_text(&s.to_text()).unwrap();
        assert_eq!(back.pinned("living room"), Some([4; 32]));
        assert_eq!(back.pinned("den"), None);
        assert_eq!(back.to_text(), s.to_text());
    }
}
