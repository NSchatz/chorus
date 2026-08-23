//! The two checks: no settable clock on the path, and no unit missing from the
//! list.
//!
//! Both are textual. That is a deliberate limit and it is worth stating: this
//! scans source, so a clock read reached through a macro this scanner cannot
//! see, or through a name it does not know, is not caught. What it does catch
//! is every ordinary way of reading a settable clock in Rust, and every way of
//! quietly moving one off the list, which is what the check is for. A cleverer
//! check would be a compiler plugin, and a compiler plugin is a dependency.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt;
use std::path::{Path, PathBuf};

use crate::list::AudioPathList;

/// The names that mean "read a clock a human or an NTP daemon can move".
///
/// `SystemTime` and `UNIX_EPOCH` are the standard library's settable clock.
/// The libc and chrono spellings are here because the day this repository
/// grows a dependency or an FFI call is exactly the day this check has to
/// already know the words.
pub const SETTABLE_CLOCK_NAMES: &[&str] = &[
    "SystemTime::now",
    "UNIX_EPOCH",
    "CLOCK_REALTIME",
    "gettimeofday",
    "clock_gettime(CLOCK_REALTIME",
    "chrono::Utc::now",
    "chrono::Local::now",
    "OffsetDateTime::now_utc",
    "OffsetDateTime::now_local",
    "time::SystemTime",
];

/// One place a listed unit reads a settable clock.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClockRead {
    /// The unit, as the list names it.
    pub unit: String,
    /// One-based line number.
    pub line: usize,
    /// The name that matched.
    pub name: String,
    /// The line itself, trimmed.
    pub text: String,
}

/// One first-party unit a listed unit depends on that the list does not
/// account for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissingUnit {
    /// The unit that is not accounted for.
    pub unit: String,
    /// The listed unit that reaches it.
    pub reached_from: String,
    /// How it is reached.
    pub how: String,
}

/// Everything the scan found.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Finding {
    /// Listed units that read a settable clock.
    pub clock_reads: Vec<ClockRead>,
    /// Units the list should have accounted for and did not.
    pub missing: Vec<MissingUnit>,
    /// Listed or excluded units that do not exist in the tree.
    pub absent: Vec<String>,
    /// How many units were reached, for the report.
    pub units_reached: usize,
}

impl Finding {
    /// Whether the tree passes both checks.
    pub fn ok(&self) -> bool {
        self.clock_reads.is_empty() && self.missing.is_empty() && self.absent.is_empty()
    }
}

impl fmt::Display for Finding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for read in &self.clock_reads {
            writeln!(
                f,
                "FAIL settable-clock-on-the-audio-path {}:{} reads {} :: {}",
                read.unit, read.line, read.name, read.text
            )?;
        }
        for missing in &self.missing {
            writeln!(
                f,
                "FAIL unit-missing-from-the-list {} is reached from {} ({}) and is neither \
                 listed nor excluded with a reason",
                missing.unit, missing.reached_from, missing.how
            )?;
        }
        for absent in &self.absent {
            writeln!(
                f,
                "FAIL unit-does-not-exist {} is named in audio-path.conf and is not in the tree",
                absent
            )?;
        }
        if self.ok() {
            writeln!(
                f,
                "pass audio-path: {} units reached from the list, none reads a settable clock, \
                 none is unaccounted for",
                self.units_reached
            )?;
        }
        Ok(())
    }
}

/// Every workspace crate: its package name with underscores, and its root
/// source file.
fn workspace_crates(root: &Path) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let crates_dir = root.join("crates");
    let entries = match std::fs::read_dir(&crates_dir) {
        Ok(e) => e,
        Err(_) => return out,
    };
    for entry in entries.flatten() {
        let dir = entry.path();
        let manifest = dir.join("Cargo.toml");
        let text = match std::fs::read_to_string(&manifest) {
            Ok(t) => t,
            Err(_) => continue,
        };
        let name = text
            .lines()
            .find_map(|l| l.trim().strip_prefix("name = "))
            .map(|v| v.trim().trim_matches('"').to_string());
        let (Some(name), Ok(relative)) = (name, dir.strip_prefix(root)) else {
            continue;
        };
        let lib = relative.join("src/lib.rs");
        if root.join(&lib).exists() {
            out.insert(
                name.replace('-', "_"),
                lib.to_string_lossy().replace('\\', "/"),
            );
        }
    }
    out
}

/// Resolve a `mod name;` inside `unit` to the file it declares.
fn module_file(root: &Path, unit: &str, module: &str) -> Option<String> {
    let unit_path = PathBuf::from(unit);
    let dir = unit_path.parent()?;
    let stem = unit_path.file_stem()?.to_string_lossy().to_string();
    // A file that is not a crate root or a `mod.rs` owns a directory of its
    // own name.
    let base = if stem == "lib" || stem == "main" || stem == "mod" {
        dir.to_path_buf()
    } else {
        dir.join(&stem)
    };
    for candidate in [
        base.join(format!("{}.rs", module)),
        base.join(module).join("mod.rs"),
    ] {
        if root.join(&candidate).exists() {
            return Some(candidate.to_string_lossy().replace('\\', "/"));
        }
    }
    None
}

/// Read a unit's source, or `None` if it is not in the tree.
fn read_unit(root: &Path, unit: &str) -> Option<String> {
    std::fs::read_to_string(root.join(unit)).ok()
}

/// Strip line comments so that a name inside prose is not a finding.
///
/// Doc comments in this repository quote the very names being looked for, so
/// scanning them would make the check fire on its own documentation.
fn code_only(line: &str) -> &str {
    match line.find("//") {
        Some(at) => &line[..at],
        None => line,
    }
}

/// Run both checks against the tree at `root`.
pub fn scan(root: &Path, list: &AudioPathList) -> Finding {
    let mut finding = Finding::default();
    let crates = workspace_crates(root);

    for unit in list.on_path.iter().chain(list.excluded.iter().map(|e| &e.unit)) {
        if read_unit(root, unit).is_none() {
            finding.absent.push(unit.clone());
        }
    }

    let mut queue: VecDeque<String> = list.on_path.iter().cloned().collect();
    let mut visited: BTreeSet<String> = BTreeSet::new();

    while let Some(unit) = queue.pop_front() {
        if !visited.insert(unit.clone()) {
            continue;
        }
        let source = match read_unit(root, &unit) {
            Some(s) => s,
            None => continue,
        };

        // Check one: a settable clock.
        for (index, raw) in source.lines().enumerate() {
            let line = code_only(raw);
            for name in SETTABLE_CLOCK_NAMES {
                if line.contains(name) {
                    finding.clock_reads.push(ClockRead {
                        unit: unit.clone(),
                        line: index + 1,
                        name: (*name).to_string(),
                        text: raw.trim().to_string(),
                    });
                }
            }
        }

        // Check two: everything this unit reaches.
        for raw in source.lines() {
            let line = code_only(raw).trim();
            if let Some(rest) = line
                .strip_prefix("mod ")
                .or_else(|| line.strip_prefix("pub mod "))
            {
                if let Some(name) = rest.strip_suffix(';') {
                    let name = name.trim();
                    if let Some(file) = module_file(root, &unit, name) {
                        if list.is_on_path(&file) {
                            queue.push_back(file);
                        } else if !list.accounts_for(&file) {
                            finding.missing.push(MissingUnit {
                                unit: file,
                                reached_from: unit.clone(),
                                how: format!("mod {};", name),
                            });
                        }
                    }
                }
            }
            for (crate_name, crate_root) in &crates {
                let uses = line.contains(&format!("use {}::", crate_name))
                    || line.contains(&format!("{}::", crate_name));
                if uses && crate_root != &unit {
                    if list.is_on_path(crate_root) {
                        queue.push_back(crate_root.clone());
                    } else if !list.accounts_for(crate_root) {
                        finding.missing.push(MissingUnit {
                            unit: crate_root.clone(),
                            reached_from: unit.clone(),
                            how: format!("uses crate {}", crate_name),
                        });
                    }
                }
            }
        }
    }

    finding.units_reached = visited.len();
    finding.missing.sort();
    finding.missing.dedup();
    finding.absent.sort();
    finding.absent.dedup();
    finding
}

/// The repository root, found by walking up from this crate's manifest.
pub fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("this crate lives at crates/<name> under the repository root")
        .to_path_buf()
}

impl PartialOrd for MissingUnit {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for MissingUnit {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (&self.unit, &self.reached_from, &self.how).cmp(&(
            &other.unit,
            &other.reached_from,
            &other.how,
        ))
    }
}
