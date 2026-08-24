//! The two audio-path checks: no settable clock on the path, and no unit
//! missing from the list. The third check this crate runs, the CPU-time-bound
//! ordering one, lives in [`crate::realtime`] and borrows this module's line
//! reading.
//!
//! Both are textual. That is a deliberate limit and it is worth stating: this
//! scans source, so a clock read reached through a macro this scanner cannot
//! see, or through a name it does not know, is not caught. What it does catch
//! is every ordinary way of reading a settable clock in Rust, and every way of
//! quietly moving one off the list, which is what the check is for. A cleverer
//! check would be a compiler plugin, and a compiler plugin is a dependency.
//!
//! # What "every ordinary way" means, exactly
//!
//! A module declaration is read in every spelling Rust allows - `mod x;`,
//! `pub mod x;`, and every restricted visibility (`pub(crate)`, `pub(super)`,
//! `pub(self)`, `pub(in a::b)`), with or without an attribute ahead of it on
//! the same line. Matching only the first two would leave the completeness
//! half of the check two words away from green, which is the opposite of what
//! it is for.
//!
//! A line comment is found where it starts, which is the first `//` that is
//! not inside a string or a character literal. Cutting at the first `//`
//! anywhere would hide the rest of a line holding a URL in a string from both
//! checks.
//!
//! # The limits that remain, stated rather than discovered
//!
//! This is a line scanner. A string literal that spans lines and a `/* */`
//! block comment are not tracked across lines: inside either, a line is read
//! as code. Both failures are in the safe direction - the check fires where it
//! need not have, rather than staying quiet where it should have fired - and
//! a false positive is one exclusion line away from being answered, in the
//! file where it belongs on the record.

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
///
/// The comment starts at the first `//` that is **not** inside a string or a
/// character literal. `let url = "https://example.invalid/x";` keeps its whole
/// line: truncating there would hide everything after it from both checks, and
/// a check with a blind spot that a URL can open is not a check.
fn code_only(line: &str) -> &str {
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if let Some((body, hashes)) = raw_string_start(bytes, i) {
            i = skip_raw_string(bytes, body, hashes);
            continue;
        }
        match bytes[i] {
            // The only cut. `line[..i]` is on a character boundary because a
            // `/` byte cannot occur inside a multi-byte UTF-8 sequence.
            b'/' if bytes.get(i + 1) == Some(&b'/') => return &line[..i],
            b'"' => i = skip_quoted(bytes, i + 1),
            b'\'' => i = skip_char_literal(bytes, i),
            _ => i += 1,
        }
    }
    line
}

/// The code of a line with the line comment dropped **and** every string and
/// character literal blanked out.
///
/// The opposite treatment of literals from [`code_only`], on purpose, because
/// the two checks want opposite things from them. A settable clock read hidden
/// after a URL in a string is a real clock read, so `code_only` keeps literals.
/// A name that occurs only inside a literal is prose, and the real-time
/// ordering check has to see it that way: this repository names the very
/// functions it searches for in doc comments, error messages and the checker's
/// own constants, and a check that fired on those would fire on its own
/// documentation.
///
/// A blanked literal keeps its length, so nothing about the rest of the line
/// moves. A lifetime (`&'a str`, `'static`) is not a literal and is left
/// alone.
pub fn code_outside_literals(line: &str) -> String {
    let code = code_only(line);
    let bytes = code.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if let Some((body, hashes)) = raw_string_start(bytes, i) {
            let end = skip_raw_string(bytes, body, hashes);
            out.resize(out.len() + (end - i), b' ');
            i = end;
            continue;
        }
        match bytes[i] {
            b'"' => {
                let end = skip_quoted(bytes, i + 1);
                out.resize(out.len() + (end - i), b' ');
                i = end;
            }
            b'\'' => {
                let end = skip_char_literal(bytes, i);
                if end > i + 1 {
                    out.resize(out.len() + (end - i), b' ');
                    i = end;
                } else {
                    // A lifetime, not a literal: it is ordinary code.
                    out.push(bytes[i]);
                    i += 1;
                }
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    // Every cut is at an ASCII quote byte, which cannot be part of a multi-byte
    // UTF-8 sequence, so each copied region is whole.
    String::from_utf8(out).unwrap_or_default()
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// If a raw string opens at `i`, where its body starts and how many hashes
/// close it. Covers `r"`, `r#"`, `br"` and `br#"`, and refuses a `r` that is
/// part of a longer name.
fn raw_string_start(bytes: &[u8], i: usize) -> Option<(usize, usize)> {
    if i > 0 && is_ident_byte(bytes[i - 1]) {
        return None;
    }
    let mut j = i;
    if bytes.get(j) == Some(&b'b') {
        j += 1;
    }
    if bytes.get(j) != Some(&b'r') {
        return None;
    }
    j += 1;
    let mut hashes = 0;
    while bytes.get(j) == Some(&b'#') {
        hashes += 1;
        j += 1;
    }
    if bytes.get(j) != Some(&b'"') {
        return None;
    }
    Some((j + 1, hashes))
}

/// Past the end of a raw string opened with `hashes` hashes, or the end of the
/// line if it does not close on this one.
fn skip_raw_string(bytes: &[u8], body: usize, hashes: usize) -> usize {
    let mut i = body;
    while i < bytes.len() {
        if bytes[i] == b'"' && bytes[i + 1..].iter().take(hashes).filter(|b| **b == b'#').count() == hashes
        {
            return i + 1 + hashes;
        }
        i += 1;
    }
    bytes.len()
}

/// Past the end of an ordinary string opened just before `i`, or the end of
/// the line.
fn skip_quoted(bytes: &[u8], mut i: usize) -> usize {
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 2,
            b'"' => return i + 1,
            _ => i += 1,
        }
    }
    bytes.len()
}

/// Past the end of a character literal starting at `i`, or past the quote
/// itself when what starts there is a lifetime.
///
/// `&'a str` and `'static` are not literals, and reading them as one would
/// swallow the rest of the line.
fn skip_char_literal(bytes: &[u8], i: usize) -> usize {
    if let Some(&next) = bytes.get(i + 1) {
        if is_ident_byte(next) && bytes.get(i + 2) != Some(&b'\'') {
            return i + 1;
        }
    }
    let mut j = i + 1;
    while j < bytes.len() {
        match bytes[j] {
            b'\\' => j += 2,
            b'\'' => return j + 1,
            _ => j += 1,
        }
    }
    bytes.len()
}

/// Drop any attributes written ahead of an item on the same line, so that
/// `#[cfg(unix)] pub(crate) mod x;` is read as the declaration it is.
fn strip_attributes(line: &str) -> &str {
    let mut line = line;
    loop {
        let rest = match line.strip_prefix("#[").or_else(|| line.strip_prefix("#![")) {
            Some(rest) => rest,
            None => return line,
        };
        let mut depth = 1usize;
        let mut close = None;
        for (at, c) in rest.char_indices() {
            match c {
                '[' => depth += 1,
                ']' => {
                    depth -= 1;
                    if depth == 0 {
                        close = Some(at);
                        break;
                    }
                }
                _ => {}
            }
        }
        match close {
            Some(at) => line = rest[at + 1..].trim_start(),
            None => return line,
        }
    }
}

/// What follows a leading visibility, whatever it is, or `None` if the line
/// starts with a word that only looks like `pub`.
///
/// `pub`, `pub(crate)`, `pub(super)`, `pub(self)` and `pub(in a::b)` are all
/// visibilities a module can be declared with, and every one of them puts the
/// module's file in the tree just the same.
fn after_visibility(line: &str) -> Option<&str> {
    let Some(after) = line.strip_prefix("pub") else {
        return Some(line);
    };
    match after.chars().next() {
        // `pub` and nothing else, or `public_holiday`: not a visibility.
        None => None,
        Some(c) if c == '(' || c.is_whitespace() => {
            let after = after.trim_start();
            match after.strip_prefix('(') {
                Some(inner) => {
                    let close = inner.find(')')?;
                    Some(inner[close + 1..].trim_start())
                }
                None => Some(after),
            }
        }
        Some(_) => None,
    }
}

/// The module a line declares, if it declares one, in any spelling.
///
/// An inline `mod x { ... }` declares no file and is not one of these: the
/// trailing semicolon is what says "the body is in another file", which is the
/// only case that can put a unit on the path without the list noticing.
fn module_declaration(line: &str) -> Option<&str> {
    let rest = after_visibility(strip_attributes(line.trim()))?;
    let rest = rest.strip_prefix("mod")?;
    if !rest.starts_with(char::is_whitespace) {
        return None;
    }
    let name = rest.trim().strip_suffix(';')?.trim();
    let name = name.strip_prefix("r#").unwrap_or(name);
    if name.is_empty() || !name.bytes().all(is_ident_byte) {
        return None;
    }
    Some(name)
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
            if let Some(name) = module_declaration(line) {
                if let Some(file) = module_file(root, &unit, name) {
                    if list.is_on_path(&file) {
                        queue.push_back(file);
                    } else if !list.accounts_for(&file) {
                        finding.missing.push(MissingUnit {
                            unit: file,
                            reached_from: unit.clone(),
                            // The declaration as written, so a reader sees the
                            // spelling that put the unit there.
                            how: line.to_string(),
                        });
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_spelling_of_a_module_declaration_is_a_module_declaration() {
        for line in [
            "mod x;",
            "pub mod x;",
            "pub(crate) mod x;",
            "pub(super) mod x;",
            "pub(self) mod x;",
            "pub(in crate::a::b) mod x;",
            "pub (crate) mod x;",
            "#[cfg(unix)] pub(crate) mod x;",
            "#[cfg(any(unix, windows))] mod x;",
            "   pub(crate)   mod   x  ;",
        ] {
            assert_eq!(module_declaration(line), Some("x"), "{}", line);
        }
    }

    #[test]
    fn things_that_are_not_a_module_declaration_are_not_read_as_one() {
        for line in [
            // An inline module declares no file of its own.
            "mod x {",
            "pub mod x {",
            // Words that merely start the same way.
            "modest();",
            "public_mod x;",
            "let mode = x;",
            // Not a declaration at all.
            "use crate::x;",
            "",
        ] {
            assert_eq!(module_declaration(line), None, "{}", line);
        }
    }

    #[test]
    fn a_comment_is_cut_and_a_string_is_not() {
        assert_eq!(code_only("let a = 1; // SystemTime::now()"), "let a = 1; ");
        assert_eq!(code_only("//! SystemTime::now()"), "");
        // The finding this test exists for: a `//` inside a literal used to
        // hide everything after it from both checks.
        assert_eq!(
            code_only(r#"let u = "https://example.invalid"; let t = SystemTime::now();"#),
            r#"let u = "https://example.invalid"; let t = SystemTime::now();"#
        );
        assert_eq!(
            code_only(r##"let u = r#"https://x"#; mod hidden;"##),
            r##"let u = r#"https://x"#; mod hidden;"##
        );
        // A lifetime is not an unterminated character literal.
        assert_eq!(
            code_only("fn f<'a>(s: &'a str) {} // gettimeofday"),
            "fn f<'a>(s: &'a str) {} "
        );
        assert_eq!(code_only(r"let c = '\''; // UNIX_EPOCH"), r"let c = '\''; ");
        assert_eq!(code_only(r#"let c = '"'; // UNIX_EPOCH"#), r#"let c = '"'; "#);
        // An escaped quote does not end the string early.
        assert_eq!(
            code_only(r#"let s = "a\" // b"; mod hidden;"#),
            r#"let s = "a\" // b"; mod hidden;"#
        );
    }

    #[test]
    fn a_clock_read_hidden_behind_a_url_in_a_string_is_still_found() {
        let line = code_only(r#"const D: &str = "see https://example.invalid"; SystemTime::now();"#);
        assert!(
            SETTABLE_CLOCK_NAMES
                .iter()
                .any(|name| line.contains(name)),
            "{}",
            line
        );
    }
}
