//! Reading `audio-path.conf`.
//!
//! The format is the smallest thing that carries what the check needs: two
//! sections, one path per line, and an exclusion carrying its reason on the
//! same line as the path it excuses. A reason is mandatory, because an
//! exclusion without one is just a shorter list.

use std::collections::BTreeMap;
use std::fmt;

/// The file this crate reads, relative to the repository root.
pub const LIST_FILE: &str = "audio-path.conf";

/// A unit deliberately left off the path, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Excluded {
    /// Path from the repository root.
    pub unit: String,
    /// One line saying why it is safe to leave off.
    pub reason: String,
}

/// The committed enumeration.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AudioPathList {
    /// Units on the audio or timestamp path.
    pub on_path: Vec<String>,
    /// Units deliberately off it, with reasons.
    pub excluded: Vec<Excluded>,
}

/// Why the list could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListError {
    /// One-based line number.
    pub line: usize,
    /// What was wrong.
    pub detail: String,
}

impl fmt::Display for ListError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} line {}: {}", LIST_FILE, self.line, self.detail)
    }
}

impl std::error::Error for ListError {}

impl AudioPathList {
    /// Parse the committed list.
    pub fn parse(text: &str) -> Result<AudioPathList, ListError> {
        let mut list = AudioPathList::default();
        let mut section: Option<&str> = None;
        let mut seen: BTreeMap<String, usize> = BTreeMap::new();

        for (index, raw) in text.lines().enumerate() {
            let line = match raw.find('#') {
                Some(at) => &raw[..at],
                None => raw,
            }
            .trim();
            if line.is_empty() {
                continue;
            }
            if let Some(name) = line.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
                section = match name {
                    "on-path" => Some("on-path"),
                    "excluded" => Some("excluded"),
                    other => {
                        return Err(ListError {
                            line: index + 1,
                            detail: format!("unknown section '[{}]'", other),
                        })
                    }
                };
                continue;
            }
            let section = section.ok_or_else(|| ListError {
                line: index + 1,
                detail: "an entry before any section header".to_string(),
            })?;

            match section {
                "on-path" => {
                    if line.contains('=') {
                        return Err(ListError {
                            line: index + 1,
                            detail: "an on-path entry is a bare path, with no reason".to_string(),
                        });
                    }
                    if let Some(first) = seen.insert(line.to_string(), index + 1) {
                        return Err(ListError {
                            line: index + 1,
                            detail: format!("'{}' also appears on line {}", line, first),
                        });
                    }
                    list.on_path.push(line.to_string());
                }
                _ => {
                    let (unit, reason) = line.split_once('=').ok_or_else(|| ListError {
                        line: index + 1,
                        detail: "an exclusion is 'path = reason'; a reason is not optional, \
                                 because an exclusion without one is just a shorter list"
                            .to_string(),
                    })?;
                    let unit = unit.trim().to_string();
                    let reason = reason.trim().to_string();
                    if reason.is_empty() {
                        return Err(ListError {
                            line: index + 1,
                            detail: format!("'{}' is excluded with an empty reason", unit),
                        });
                    }
                    if let Some(first) = seen.insert(unit.clone(), index + 1) {
                        return Err(ListError {
                            line: index + 1,
                            detail: format!("'{}' also appears on line {}", unit, first),
                        });
                    }
                    list.excluded.push(Excluded { unit, reason });
                }
            }
        }
        Ok(list)
    }

    /// Whether `unit` is accounted for, either way.
    pub fn accounts_for(&self, unit: &str) -> bool {
        self.is_on_path(unit) || self.excluded.iter().any(|e| e.unit == unit)
    }

    /// Whether `unit` is on the path.
    ///
    /// Distinct from [`accounts_for`] on purpose: an excluded unit is
    /// accounted for and is **not** followed. Following it would drag its own
    /// dependencies onto the path and would scan it for the very thing its
    /// reason excuses.
    ///
    /// [`accounts_for`]: AudioPathList::accounts_for
    pub fn is_on_path(&self, unit: &str) -> bool {
        self.on_path.iter().any(|u| u == unit)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_list_with_both_sections_parses() {
        let text = "# a comment\n\
                    [on-path]\n\
                    crates/audio/src/lib.rs\n\
                    crates/audio/src/chunker.rs\n\
                    \n\
                    [excluded]\n\
                    crates/client-linux/src/delaylog.rs = writes the record, reads nothing\n";
        let list = AudioPathList::parse(text).unwrap();
        assert_eq!(list.on_path.len(), 2);
        assert_eq!(list.excluded.len(), 1);
        assert_eq!(
            list.excluded[0].reason,
            "writes the record, reads nothing"
        );
        assert!(list.accounts_for("crates/audio/src/chunker.rs"));
        assert!(list.accounts_for("crates/client-linux/src/delaylog.rs"));
        assert!(!list.accounts_for("crates/audio/src/clock.rs"));
    }

    #[test]
    fn an_exclusion_without_a_reason_is_refused() {
        let text = "[excluded]\ncrates/x/src/y.rs\n";
        let err = AudioPathList::parse(text).unwrap_err();
        assert!(err.detail.contains("a reason is not optional"));
    }

    #[test]
    fn an_exclusion_with_an_empty_reason_is_refused() {
        let text = "[excluded]\ncrates/x/src/y.rs =   \n";
        let err = AudioPathList::parse(text).unwrap_err();
        assert!(err.detail.contains("empty reason"));
    }

    #[test]
    fn a_unit_that_is_both_listed_and_excluded_is_refused() {
        let text = "[on-path]\ncrates/x/src/y.rs\n[excluded]\ncrates/x/src/y.rs = both\n";
        let err = AudioPathList::parse(text).unwrap_err();
        assert!(err.detail.contains("also appears on line"));
    }

    #[test]
    fn an_entry_before_any_section_is_refused() {
        let err = AudioPathList::parse("crates/x/src/y.rs\n").unwrap_err();
        assert!(err.detail.contains("before any section"));
    }

    #[test]
    fn an_unknown_section_is_refused() {
        let err = AudioPathList::parse("[maybe]\n").unwrap_err();
        assert!(err.detail.contains("unknown section"));
    }
}
