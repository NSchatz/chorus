//! The directory name of a target key.
//!
//! Soloist keeps a Spotify Connect identity and its stored session in its
//! data directory ("Use the same data directory across restarts to keep the
//! same device identity and stored Spotify Connect session", Soloist's
//! getting-started page, read 2026-10-03). chorus gives every target its own
//! directory, named from the target's key, so a receiver that is handed a
//! target it served before, or a live group that forms again, is the same
//! device in the Spotify app.
//!
//! A key is the UPnP renderer's (`chorus_upnp::uuid::Target`): `room:<id>`,
//! `group:<id>`, or `live:<member ids, sorted, joined by +>`. Ids are
//! catalog identifiers: lower-case letters, digits and hyphens. The name
//! replaces the colon with a hyphen and each plus with an underscore:
//!
//! | key | directory name |
//! |---|---|
//! | `room:kitchen` | `room-kitchen` |
//! | `group:downstairs` | `group-downstairs` |
//! | `live:den+kitchen` | `live-den_kitchen` |
//!
//! The mapping is injective: the kind is everything before the first hyphen
//! (no kind contains one), and an underscore can only be a separator (no id
//! contains one). [`key_of`] is the inverse.

/// The longest directory name [`dir_name`] returns: the usual limit of one
/// path component on Linux file systems (`NAME_MAX`, 255 bytes).
pub const MAX_NAME: usize = 255;

/// Why a key has no directory name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyError {
    /// Not `room:`, `group:` or `live:` followed by ids.
    NotAKey,
    /// An id that is empty or has a character outside `[a-z0-9-]`.
    BadId(String),
    /// The name would pass [`MAX_NAME`] bytes (a live group of very many
    /// rooms).
    TooLong,
}

impl std::fmt::Display for KeyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            KeyError::NotAKey => write!(f, "not a room:, group: or live: key"),
            KeyError::BadId(id) => write!(f, "{id:?} is not an identifier"),
            KeyError::TooLong => write!(f, "the directory name would pass {MAX_NAME} bytes"),
        }
    }
}

impl std::error::Error for KeyError {}

fn is_id(text: &str) -> bool {
    !text.is_empty()
        && text
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// The kind of a target key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Kind {
    /// `room:<id>`.
    Room,
    /// `group:<id>`: a saved group.
    Group,
    /// `live:<ids>`: a live group.
    Live,
}

/// The kind a key's prefix names, if it has one of the three.
pub fn kind_of(key: &str) -> Option<Kind> {
    match key.split_once(':')?.0 {
        "room" => Some(Kind::Room),
        "group" => Some(Kind::Group),
        "live" => Some(Kind::Live),
        _ => None,
    }
}

/// The directory name of a target key.
pub fn dir_name(key: &str) -> Result<String, KeyError> {
    let (kind, rest) = key.split_once(':').ok_or(KeyError::NotAKey)?;
    let ids: Vec<&str> = match kind {
        "room" | "group" => vec![rest],
        "live" => rest.split('+').collect(),
        _ => return Err(KeyError::NotAKey),
    };
    if let Some(bad) = ids.iter().find(|id| !is_id(id)) {
        return Err(KeyError::BadId(bad.to_string()));
    }
    let name = format!("{kind}-{}", ids.join("_"));
    if name.len() > MAX_NAME {
        return Err(KeyError::TooLong);
    }
    Ok(name)
}

/// The key a directory name was made from, if [`dir_name`] made it.
pub fn key_of(name: &str) -> Option<String> {
    let (kind, rest) = name.split_once('-')?;
    let key = match kind {
        "room" | "group" if !rest.contains('_') => format!("{kind}:{rest}"),
        "live" => format!("live:{}", rest.replace('_', "+")),
        _ => return None,
    };
    (dir_name(&key).as_deref() == Ok(name)).then_some(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_three_kinds() {
        assert_eq!(dir_name("room:kitchen").as_deref(), Ok("room-kitchen"));
        assert_eq!(
            dir_name("group:downstairs").as_deref(),
            Ok("group-downstairs")
        );
        assert_eq!(
            dir_name("live:den+kitchen").as_deref(),
            Ok("live-den_kitchen")
        );
        assert_eq!(dir_name("live:a+b-2+c").as_deref(), Ok("live-a_b-2_c"));
        assert_eq!(dir_name("live:solo").as_deref(), Ok("live-solo"));
        assert_eq!(kind_of("room:kitchen"), Some(Kind::Room));
        assert_eq!(kind_of("group:x"), Some(Kind::Group));
        assert_eq!(kind_of("live:a+b"), Some(Kind::Live));
        assert_eq!(kind_of("player:1"), None);
        assert_eq!(kind_of("room"), None);
        assert!(Kind::Room < Kind::Group && Kind::Group < Kind::Live);
    }

    #[test]
    fn what_is_not_a_key() {
        for key in [
            "",
            "kitchen",
            "player:1",
            "soloist:r0",
            "Room:kitchen",
            "room-kitchen",
        ] {
            assert_eq!(dir_name(key), Err(KeyError::NotAKey), "{key}");
        }
        for (key, id) in [
            ("room:", ""),
            ("room:Kitchen", "Kitchen"),
            ("room:a/b", "a/b"),
            ("room:..", ".."),
            ("room:a_b", "a_b"),
            ("room:a+b", "a+b"),
            ("group:a:b", "a:b"),
            ("live:a++b", ""),
            ("live:a+", ""),
            ("live:a+b c", "b c"),
            ("room:\u{e9}", "\u{e9}"),
        ] {
            assert_eq!(dir_name(key), Err(KeyError::BadId(id.into())), "{key}");
        }
        let long = format!("live:{}", vec!["a".repeat(32); 8].join("+"));
        assert_eq!(dir_name(&long), Err(KeyError::TooLong));
    }

    /// A name never leaves its directory: no separator, no dot, no NUL.
    #[test]
    fn a_name_is_one_safe_path_component() {
        for key in ["room:a", "group:a-b", "live:a+b+c", "room:-", "room:--x"] {
            let name = dir_name(key).unwrap();
            assert!(name
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_'));
            assert!(!name.starts_with('-') && !name.starts_with('.'));
        }
    }

    /// Injective, shown two ways: the inverse recovers every key, and over
    /// every key of a small alphabet no two share a name.
    #[test]
    fn the_mapping_is_injective() {
        let ids = [
            "a", "b", "-", "a-b", "a-", "-a", "room", "live", "group", "a-b-c", "0",
        ];
        let mut keys = Vec::new();
        for id in ids {
            keys.push(format!("room:{id}"));
            keys.push(format!("group:{id}"));
            keys.push(format!("live:{id}"));
            for other in ids {
                keys.push(format!("live:{id}+{other}"));
                for third in ids {
                    keys.push(format!("live:{id}+{other}+{third}"));
                }
            }
        }
        let mut seen = std::collections::BTreeMap::new();
        for key in &keys {
            let name = dir_name(key).unwrap();
            assert_eq!(key_of(&name).as_ref(), Some(key));
            if let Some(previous) = seen.insert(name.clone(), key.clone()) {
                panic!("{previous} and {key} share {name}");
            }
        }
        assert_eq!(seen.len(), keys.len());
        for name in [
            "",
            "room",
            "room-",
            "room-a_b",
            "player-a",
            "live-a__b",
            "Room-a",
        ] {
            assert_eq!(key_of(name), None, "{name}");
        }
    }
}
