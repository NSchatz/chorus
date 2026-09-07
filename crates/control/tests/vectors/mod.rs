//! Reading `fixtures/control/`.
//!
//! Under `tests/`, so nothing in the shipped library or either binary can
//! reach it.

#![allow(dead_code)]

use std::path::{Path, PathBuf};

/// The repository root, found from this crate's manifest.
pub fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("this crate lives at crates/<name> under the repository root")
        .to_path_buf()
}

/// Where the committed control vectors are.
pub fn fixture_dir() -> PathBuf {
    repository_root().join("fixtures/control")
}

/// Every vector's name, discovered from the directory and sorted, so that
/// adding a vector is adding two files and registering nothing.
pub fn vector_names() -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(fixture_dir())
        .expect("fixtures/control is committed")
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("fields") {
                return None;
            }
            path.file_stem()
                .and_then(|s| s.to_str())
                .map(|s| s.to_string())
        })
        .collect();
    names.sort();
    assert!(!names.is_empty(), "fixtures/control holds no vector at all");
    names
}

/// The bytes of one vector's message, which is its file with the one trailing
/// newline removed.
pub fn read_json(name: &str) -> String {
    let path = fixture_dir().join(format!("{}.json", name));
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{} is committed and readable: {}", path.display(), e));
    let stripped = text.strip_suffix('\n').unwrap_or_else(|| {
        panic!(
            "{} must end with exactly one newline, so that the message bytes are the file \
             minus that newline",
            path.display()
        )
    });
    assert!(
        !stripped.ends_with('\n'),
        "{} ends with more than one newline",
        path.display()
    );
    stripped.to_string()
}

/// The `key = value` pairs of one vector's canonical input.
pub struct Fields {
    pairs: Vec<(String, String)>,
    name: String,
}

impl Fields {
    /// One value, which must be there.
    pub fn get(&self, key: &str) -> String {
        self.pairs
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
            .unwrap_or_else(|| panic!("{}.fields has no '{}'", self.name, key))
    }

    /// Whether a key is present.
    pub fn has(&self, key: &str) -> bool {
        self.pairs.iter().any(|(k, _)| k == key)
    }

    /// Every pair, in file order.
    pub fn pairs(&self) -> impl Iterator<Item = (&str, &str)> {
        self.pairs.iter().map(|(k, v)| (k.as_str(), v.as_str()))
    }
}

/// Read one vector's `.fields`.
///
/// A `#` only starts a comment at the beginning of a line: a detail string in
/// a refusal vector may contain one, and cutting the line there would change
/// the value the vector declares.
pub fn read_fields(name: &str) -> Fields {
    let path = fixture_dir().join(format!("{}.fields", name));
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{} is committed and readable: {}", path.display(), e));
    let mut pairs = Vec::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (key, value) = line
            .split_once('=')
            .unwrap_or_else(|| panic!("{}: '{}' is not 'key = value'", path.display(), line));
        pairs.push((key.trim().to_string(), value.trim().to_string()));
    }
    Fields {
        pairs,
        name: name.to_string(),
    }
}
