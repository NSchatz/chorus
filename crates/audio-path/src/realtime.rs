//! The third check: every real-time acquisition applies the CPU-time bound
//! before it takes the scheduling policy.
//!
//! # The invariant, and why it needs a check rather than a sentence
//!
//! `sched(7)`: "A nonblocking infinite loop in a thread scheduled under the
//! SCHED_FIFO, SCHED_RR, or SCHED_DEADLINE policy can potentially block all
//! other threads from accessing the CPU forever." `RLIMIT_RTTIME` is the
//! documented bound on that, and a bound applied *after* the policy leaves
//! exactly the window the manual warns about: between the two calls the thread
//! is real-time and unbounded, and a spin in there starves the host.
//!
//! The invariant held in this tree before this check existed, and
//! `docs/verification-record.md` said so. A true statement about an undefended
//! property is a promise, not a safety property: the next refactor moves one
//! line and nothing notices. So the ordering is graded here, in the ordinary
//! suite, with no device and no privilege.
//!
//! # What counts as a real-time acquisition
//!
//! A **call** to the one function in this repository that puts a thread under
//! a real-time scheduling policy, and a **call** to the one that applies the
//! CPU-time bound. Both names are in [`POLICY_ACQUISITION_NAMES`] and
//! [`CPU_TIME_BOUND_NAMES`]. A definition of either (`fn` followed by the
//! name) is not a call, and neither is a name in an import list, which is a
//! name without a call after it.
//!
//! A site holds when a CPU-time bound call appears **earlier in the same
//! function**. The enclosing function is the nearest `fn` declaration above
//! the site; a call inside a closure is read as belonging to the function that
//! contains the closure.
//!
//! # Why the sites are enumerated in a committed file
//!
//! Same reason `audio-path.conf` exists: a check that only grades the sites it
//! happens to find cannot tell "there are two acquisitions and both are fine"
//! from "there is a third one somewhere I did not look". So
//! `real-time-acquisitions.conf` names every unit that takes a real-time
//! policy, an acquisition in a unit it does not name is reported as
//! unaccounted for, and a named unit that acquires nothing is reported as
//! absent.
//!
//! That file has no exclusion section, deliberately, and this is the
//! difference from `audio-path.conf`. An exclusion would be a way to answer an
//! ordering finding by writing a sentence, and there is no sentence that makes
//! an unbounded real-time thread safe. Adding a unit to the list does not
//! excuse it from the ordering check; it subjects it to one.
//!
//! # The limits that remain, stated rather than discovered
//!
//! This is a line scanner, like [`crate::scan`], and it inherits that module's
//! limits: a multi-line string literal and a `/* */` block comment are not
//! tracked across lines. Four limits are its own, and
//! `docs/decisions/0012-the-cpu-time-bound-goes-on-first.md` carries the same
//! list in the same order.
//!
//! Two are in the safe direction, where the check fires where it need not
//! have. A raw `sched_setscheduler` call that bypassed this repository's own
//! entry point would not be seen as an acquisition - the entry point is where
//! the wrapper lives, and every caller in this tree goes through it. And a
//! bound applied in one function for a policy taken in another is not
//! credited: the answer is to move the bound next to the acquisition, which is
//! where it belongs anyway.
//!
//! Two run the other way, towards a green that is wrong. `source_units` reads
//! `.rs` files under `crates/` and nowhere else, so an acquisition in a Rust
//! source elsewhere under the repository root would be neither graded nor
//! reported as unaccounted for; nothing outside `crates/` acquires a policy
//! today, and widening that root is the change to make when one does. And
//! because a site is credited to the nearest `fn` above it, a bound written
//! inside a closure counts for an acquisition outside that closure in the same
//! function, though the closure may never run.

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

use crate::scan::code_outside_literals;

/// The file this check reads, relative to the repository root.
pub const SITE_FILE: &str = "real-time-acquisitions.conf";

/// The names that mean "put this thread under a real-time scheduling policy".
pub const POLICY_ACQUISITION_NAMES: &[&str] = &["take_real_time_policy"];

/// The names that mean "apply the CPU-time bound that `sched(7)` documents".
pub const CPU_TIME_BOUND_NAMES: &[&str] = &["bound_real_time_cpu_time"];

/// One place the tree takes a real-time scheduling policy.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Acquisition {
    /// The unit, as a path from the repository root.
    pub unit: String,
    /// One-based line number of the acquisition.
    pub line: usize,
    /// The function that contains it.
    pub function: String,
    /// The name that matched.
    pub name: String,
    /// The line of the CPU-time bound that precedes it in the same function.
    pub bounded_at: Option<usize>,
    /// The line of a CPU-time bound that follows it in the same function,
    /// which is the ordering this check exists to catch.
    pub bound_after_at: Option<usize>,
    /// The line itself, trimmed.
    pub text: String,
}

/// Everything the real-time scan found.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RealTimeFinding {
    /// Accounted-for sites that apply the bound first. These are the examined
    /// ones the report counts.
    pub holding: Vec<Acquisition>,
    /// Sites that take the policy without applying the bound first.
    pub unbounded: Vec<Acquisition>,
    /// Sites in a unit `real-time-acquisitions.conf` does not name.
    pub unaccounted: Vec<Acquisition>,
    /// Units the list names that acquire no real-time policy in this tree.
    pub absent: Vec<String>,
}

impl RealTimeFinding {
    /// Whether the tree passes.
    pub fn ok(&self) -> bool {
        self.unbounded.is_empty() && self.unaccounted.is_empty() && self.absent.is_empty()
    }

    /// How many real-time acquisition sites the check examined, holding or
    /// not.
    pub fn sites_examined(&self) -> usize {
        self.holding.len() + self.unbounded.len() + self.unaccounted.len()
    }
}

impl fmt::Display for RealTimeFinding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for site in &self.unbounded {
            let bound = match self.bound_phrase(site) {
                Some(phrase) => phrase,
                None => "no CPU-time bound is applied anywhere in that function".to_string(),
            };
            writeln!(
                f,
                "FAIL cpu-time-bound-not-applied-first {}:{} takes a real-time scheduling policy \
                 in fn {} and {} :: {}",
                site.unit, site.line, site.function, bound, site.text
            )?;
        }
        for site in &self.unaccounted {
            writeln!(
                f,
                "FAIL unaccounted-real-time-acquisition {}:{} takes a real-time scheduling policy \
                 in fn {} and {} does not name that unit :: {}",
                site.unit, site.line, site.function, SITE_FILE, site.text
            )?;
        }
        for unit in &self.absent {
            writeln!(
                f,
                "FAIL real-time-acquisition-does-not-exist {} is named in {} and takes no \
                 real-time scheduling policy in this tree",
                unit, SITE_FILE
            )?;
        }
        if self.ok() {
            writeln!(
                f,
                "pass real-time-ordering: {} real-time acquisition sites examined, every one \
                 applies the CPU-time bound before it takes the scheduling policy",
                self.sites_examined()
            )?;
        }
        Ok(())
    }
}

impl RealTimeFinding {
    fn bound_phrase(&self, site: &Acquisition) -> Option<String> {
        site.bound_after_at
            .map(|at| format!("the CPU-time bound is applied at line {}, after it", at))
    }
}

/// The committed enumeration of the units that take a real-time policy.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AcquisitionList {
    /// Units that acquire a real-time scheduling policy.
    pub units: Vec<String>,
}

/// Why the list could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SiteListError {
    /// One-based line number.
    pub line: usize,
    /// What was wrong.
    pub detail: String,
}

impl fmt::Display for SiteListError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} line {}: {}", SITE_FILE, self.line, self.detail)
    }
}

impl std::error::Error for SiteListError {}

impl AcquisitionList {
    /// Parse the committed list.
    ///
    /// One section, `[acquisitions]`, one bare path per line, `#` starts a
    /// comment. There is no exclusion section: see this module's header for
    /// why an exclusion would be the wrong shape for this invariant.
    pub fn parse(text: &str) -> Result<AcquisitionList, SiteListError> {
        let mut list = AcquisitionList::default();
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
                    "acquisitions" => Some("acquisitions"),
                    other => {
                        return Err(SiteListError {
                            line: index + 1,
                            detail: format!(
                                "unknown section '[{}]'; this file has one section and no \
                                 exclusions, because there is no reason that makes an unbounded \
                                 real-time thread safe",
                                other
                            ),
                        })
                    }
                };
                continue;
            }
            if section.is_none() {
                return Err(SiteListError {
                    line: index + 1,
                    detail: "an entry before any section header".to_string(),
                });
            }
            if line.contains('=') {
                return Err(SiteListError {
                    line: index + 1,
                    detail: "an entry is a bare path, with no reason attached: a unit on this \
                             list is graded, not excused"
                        .to_string(),
                });
            }
            if let Some(first) = seen.insert(line.to_string(), index + 1) {
                return Err(SiteListError {
                    line: index + 1,
                    detail: format!("'{}' also appears on line {}", line, first),
                });
            }
            list.units.push(line.to_string());
        }
        Ok(list)
    }

    /// Whether the list names `unit`.
    pub fn names(&self, unit: &str) -> bool {
        self.units.iter().any(|u| u == unit)
    }
}

/// What one line of source turned out to be.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Mark {
    /// A function declaration starts here.
    FunctionStart(String),
    /// A CPU-time bound is applied here.
    Bound,
    /// A real-time scheduling policy is taken here.
    Acquire(String),
}

/// Whether `name` is *called* at some point in `code`, which has already had
/// its comments and literals removed.
///
/// A call is the name followed by `(`. A definition is excluded: the token
/// before the name is `fn`. An import list is excluded for free, because a
/// name in one is followed by `,` or `}` rather than by a parenthesis.
fn calls(code: &str, name: &str) -> bool {
    let mut from = 0;
    while let Some(at) = code[from..].find(name) {
        let start = from + at;
        let end = start + name.len();
        from = end;
        let before_ok = start == 0 || !is_ident_byte(code.as_bytes()[start - 1]);
        let after = code[end..].trim_start();
        if !before_ok || !after.starts_with('(') {
            continue;
        }
        // `fn take(...)` declares; `take(...)` calls.
        if code[..start].trim_end().ends_with("fn") {
            continue;
        }
        return true;
    }
    false
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// The name of the function a line declares, if it declares one.
///
/// Handles every leading word Rust allows before `fn` - visibility, `const`,
/// `async`, `unsafe`, `extern "C"` - by looking for `fn` as a standalone token
/// rather than by matching prefixes.
fn function_declaration(code: &str) -> Option<String> {
    let bytes = code.as_bytes();
    let mut from = 0;
    while let Some(at) = code[from..].find("fn") {
        let start = from + at;
        let end = start + 2;
        from = end;
        let before_ok = start == 0 || !is_ident_byte(bytes[start - 1]);
        if !before_ok {
            continue;
        }
        let rest = &code[end..];
        if !rest.starts_with(char::is_whitespace) {
            continue;
        }
        let rest = rest.trim_start();
        let name: String = rest
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        if name.is_empty() {
            continue;
        }
        let after = rest[name.len()..].trim_start();
        if after.starts_with('(') || after.starts_with('<') {
            return Some(name);
        }
    }
    None
}

/// Every `.rs` file under `root/crates`, as paths from `root`.
fn source_units(root: &Path) -> Vec<String> {
    let mut out = Vec::new();
    collect(root, &root.join("crates"), &mut out);
    out.sort();
    out
}

fn collect(root: &Path, dir: &Path, out: &mut Vec<String>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if entry.file_name() == "target" {
                continue;
            }
            collect(root, &path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            if let Ok(relative) = path.strip_prefix(root) {
                out.push(relative.to_string_lossy().replace('\\', "/"));
            }
        }
    }
}

/// Mark up one unit's source, one line at a time.
fn marks(source: &str) -> Vec<(usize, Mark)> {
    let mut out = Vec::new();
    for (index, raw) in source.lines().enumerate() {
        let code = code_outside_literals(raw);
        let line = index + 1;
        if let Some(name) = function_declaration(&code) {
            out.push((line, Mark::FunctionStart(name)));
        }
        if CPU_TIME_BOUND_NAMES.iter().any(|n| calls(&code, n)) {
            out.push((line, Mark::Bound));
        }
        for name in POLICY_ACQUISITION_NAMES {
            if calls(&code, name) {
                out.push((line, Mark::Acquire((*name).to_string())));
            }
        }
    }
    out
}

/// Every real-time acquisition in one unit, with the bound lines around it.
fn acquisitions_in(unit: &str, source: &str) -> Vec<Acquisition> {
    let lines: Vec<&str> = source.lines().collect();
    let marked = marks(source);

    // The function each mark belongs to, and where that function started.
    let mut current = (0usize, "<file scope>".to_string());
    let mut owner: Vec<(usize, String)> = Vec::with_capacity(marked.len());
    for (line, mark) in &marked {
        if let Mark::FunctionStart(name) = mark {
            current = (*line, name.clone());
        }
        owner.push(current.clone());
    }

    let bounds: Vec<(usize, usize)> = marked
        .iter()
        .zip(owner.iter())
        .filter(|((_, mark), _)| matches!(mark, Mark::Bound))
        .map(|((line, _), (fn_start, _))| (*fn_start, *line))
        .collect();

    let mut out = Vec::new();
    for ((line, mark), (fn_start, function)) in marked.iter().zip(owner.iter()) {
        let Mark::Acquire(name) = mark else {
            continue;
        };
        let same_function = bounds.iter().filter(|(start, _)| start == fn_start);
        let mut before = None;
        let mut after = None;
        for (_, bound_line) in same_function {
            if bound_line < line {
                before = Some(before.map_or(*bound_line, |b: usize| b.max(*bound_line)));
            } else if bound_line > line {
                after = Some(after.map_or(*bound_line, |a: usize| a.min(*bound_line)));
            }
        }
        out.push(Acquisition {
            unit: unit.to_string(),
            line: *line,
            function: function.clone(),
            name: name.clone(),
            bounded_at: before,
            bound_after_at: after,
            text: lines[*line - 1].trim().to_string(),
        });
    }
    out
}

/// Run the ordering check against the tree at `root`.
pub fn scan_real_time(root: &Path, list: &AcquisitionList) -> RealTimeFinding {
    let mut finding = RealTimeFinding::default();
    let mut acquiring: Vec<String> = Vec::new();

    for unit in source_units(root) {
        let source = match std::fs::read_to_string(root.join(&unit)) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let sites = acquisitions_in(&unit, &source);
        if sites.is_empty() {
            continue;
        }
        acquiring.push(unit.clone());
        for site in sites {
            if !list.names(&unit) {
                finding.unaccounted.push(site);
            } else if site.bounded_at.is_some() {
                finding.holding.push(site);
            } else {
                finding.unbounded.push(site);
            }
        }
    }

    for unit in &list.units {
        if !acquiring.contains(unit) {
            finding.absent.push(unit.clone());
        }
    }

    finding.holding.sort();
    finding.unbounded.sort();
    finding.unaccounted.sort();
    finding.absent.sort();
    finding
}

/// The repository root, found by walking up from this crate's manifest.
pub fn repository_root() -> PathBuf {
    crate::scan::repository_root()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_call_is_a_call_and_a_definition_is_not() {
        assert!(calls("    match take_a_policy(10) {", "take_a_policy"));
        assert!(calls("let x = take_a_policy(p)?;", "take_a_policy"));
        assert!(!calls("pub fn take_a_policy(wanted: u32) -> u32 {", "take_a_policy"));
        assert!(!calls("    fn take_a_policy(w: u32);", "take_a_policy"));
        // An import list names it without calling it.
        assert!(!calls("    take_a_policy, thread_id,", "take_a_policy"));
        assert!(!calls("use chorus_hostctl::take_a_policy;", "take_a_policy"));
        // A longer name that merely contains it is not it.
        assert!(!calls("do_not_take_a_policy_twice(1)", "take_a_policy"));
    }

    /// `function_declaration` is fed the output of `code_outside_literals`, so
    /// the cases below go through it too, which is what makes the commented
    /// and quoted ones mean anything.
    #[test]
    fn every_way_of_writing_a_function_declaration_is_one() {
        for line in [
            "fn main() {",
            "pub fn a(b: u32) {",
            "pub(crate) fn a() {",
            "    async fn a() {",
            "pub unsafe fn a() {",
            "pub const fn a() {",
            "    fn a<T: Copy>(t: T) {",
            "    fn getrlimit(resource: c_int) -> c_int;",
        ] {
            assert!(
                function_declaration(&code_outside_literals(line)).is_some(),
                "{}",
                line
            );
        }
        for line in [
            "let fnord = 1;",
            "// fn a() {",
            "/// fn a() {",
            "let s = \"fn a() {\";",
            "let f = |x| x;",
            "",
        ] {
            assert_eq!(
                function_declaration(&code_outside_literals(line)),
                None,
                "{}",
                line
            );
        }
    }

    #[test]
    fn a_name_that_occurs_only_in_prose_is_not_a_site() {
        let source = concat!(
            "//! A doc comment naming ", "take_real_time_policy", "(0).\n",
            "fn f() {\n",
            "    // A line comment naming ", "take_real_time_policy", "(0).\n",
            "    let message = \"", "take_real_time_policy", "(0)\";\n",
            "    let _ = message;\n",
            "}\n",
        );
        assert!(acquisitions_in("crates/x/src/y.rs", source).is_empty());
    }

    #[test]
    fn the_bound_before_and_the_bound_after_are_told_apart() {
        let good = concat!(
            "fn f() {\n",
            "    ", "bound_real_time_cpu_time", "(200_000)?;\n",
            "    ", "take_real_time_policy", "(20)?;\n",
            "}\n",
        );
        let sites = acquisitions_in("crates/x/src/y.rs", good);
        assert_eq!(sites.len(), 1);
        assert_eq!(sites[0].bounded_at, Some(2));
        assert_eq!(sites[0].function, "f");

        let bad = concat!(
            "fn f() {\n",
            "    ", "take_real_time_policy", "(20)?;\n",
            "    ", "bound_real_time_cpu_time", "(200_000)?;\n",
            "}\n",
        );
        let sites = acquisitions_in("crates/x/src/y.rs", bad);
        assert_eq!(sites.len(), 1);
        assert_eq!(sites[0].bounded_at, None);
        assert_eq!(sites[0].bound_after_at, Some(3));
    }

    #[test]
    fn a_bound_in_another_function_does_not_count() {
        let source = concat!(
            "fn bounded() {\n",
            "    ", "bound_real_time_cpu_time", "(200_000)?;\n",
            "}\n",
            "fn acquires() {\n",
            "    ", "take_real_time_policy", "(20)?;\n",
            "}\n",
        );
        let sites = acquisitions_in("crates/x/src/y.rs", source);
        assert_eq!(sites.len(), 1);
        assert_eq!(sites[0].bounded_at, None);
        assert_eq!(sites[0].bound_after_at, None);
        assert_eq!(sites[0].function, "acquires");
    }

    #[test]
    fn a_list_with_one_section_parses_and_a_reason_is_refused() {
        let list = AcquisitionList::parse("# a comment\n[acquisitions]\ncrates/a/src/b.rs\n")
            .unwrap();
        assert!(list.names("crates/a/src/b.rs"));
        let err = AcquisitionList::parse("[acquisitions]\ncrates/a/src/b.rs = because\n")
            .unwrap_err();
        assert!(err.detail.contains("graded, not excused"));
        let err = AcquisitionList::parse("[excluded]\n").unwrap_err();
        assert!(err.detail.contains("unknown section"));
        let err = AcquisitionList::parse("crates/a/src/b.rs\n").unwrap_err();
        assert!(err.detail.contains("before any section"));
        let err =
            AcquisitionList::parse("[acquisitions]\ncrates/a.rs\ncrates/a.rs\n").unwrap_err();
        assert!(err.detail.contains("also appears on line"));
    }
}
