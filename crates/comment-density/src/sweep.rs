// The sweep: which files are counted, what happened to each, and the report a
// reader gets. A file that could not be read or could not be tokenized stays in
// the report as itself rather than becoming a zero, because a silent zero and a
// file with no comments are the same green otherwise.

use std::fmt::Write as _;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::count::{count, format_tenths, tenths, Config, Counts};
use crate::lex::LexError;
use crate::Missing;

// Unpinned by construction: these trees are wrong on purpose, and counting them
// against the repository would leave chorus permanently red for the exact
// reason the gate exists.
pub const DEMONSTRATIONS: &str = "tools/comment-density-demonstrations";

#[derive(Debug, Clone)]
pub enum Outcome {
    Measured(Counts),
    Unreadable(String),
    Untokenizable(LexError),
}

#[derive(Debug, Clone)]
pub struct FileResult {
    pub path: String,
    pub outcome: Outcome,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    Clean,
    EmptySweep,
    Unreadable(Vec<String>),
    Untokenizable(Vec<String>),
    OverCeiling(Vec<String>),
}

impl Verdict {
    pub fn exit_code(&self) -> i32 {
        match self {
            Verdict::Clean => crate::EXIT_OK,
            Verdict::OverCeiling(_) => crate::EXIT_OVER_CEILING,
            Verdict::Untokenizable(_) => crate::EXIT_UNTOKENIZABLE,
            Verdict::Unreadable(_) => crate::EXIT_UNREADABLE,
            Verdict::EmptySweep => crate::EXIT_EMPTY_SWEEP,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Sweep {
    pub root: PathBuf,
    pub files: Vec<FileResult>,
}

impl Sweep {
    pub fn run(root: &Path, paths: Vec<String>, cfg: &Config) -> Sweep {
        let mut paths = paths;
        paths.sort();
        let files = paths
            .into_iter()
            .map(|path| {
                let outcome = match read_source(&root.join(&path)) {
                    Err(why) => Outcome::Unreadable(why),
                    Ok(src) => match count(&src, cfg) {
                        Ok(counts) => Outcome::Measured(counts),
                        Err(error) => Outcome::Untokenizable(error),
                    },
                };
                FileResult { path, outcome }
            })
            .collect();
        Sweep { root: root.to_path_buf(), files }
    }

    // Everything the ceiling applies to: read, tokenized, and not a generated
    // file. A below-minimum file is here and is judged in verdict().
    pub fn measured(&self) -> impl Iterator<Item = (&str, &Counts)> {
        self.files.iter().filter_map(|file| match &file.outcome {
            Outcome::Measured(counts) if counts.generated_marker.is_none() => {
                Some((file.path.as_str(), counts))
            }
            _ => None,
        })
    }

    pub fn aggregate(&self) -> (usize, usize) {
        self.measured()
            .fold((0, 0), |(prose, code), (_, counts)| (prose + counts.prose, code + counts.code))
    }

    pub fn verdict(&self, cfg: &Config) -> Verdict {
        if self.files.is_empty() {
            return Verdict::EmptySweep;
        }
        let unreadable: Vec<String> = self
            .files
            .iter()
            .filter(|file| matches!(file.outcome, Outcome::Unreadable(_)))
            .map(|file| file.path.clone())
            .collect();
        if !unreadable.is_empty() {
            return Verdict::Unreadable(unreadable);
        }
        let untokenizable: Vec<String> = self
            .files
            .iter()
            .filter(|file| matches!(file.outcome, Outcome::Untokenizable(_)))
            .map(|file| file.path.clone())
            .collect();
        if !untokenizable.is_empty() {
            return Verdict::Untokenizable(untokenizable);
        }
        let over: Vec<String> = self
            .measured()
            .filter(|(_, counts)| {
                counts.counted() >= cfg.minimum_counted_lines
                    && counts.tenths() > cfg.ceiling_percent * 10
            })
            .map(|(path, _)| path.to_string())
            .collect();
        if !over.is_empty() {
            return Verdict::OverCeiling(over);
        }
        Verdict::Clean
    }

    pub fn render(&self, cfg: &Config) -> String {
        let mut out = String::new();
        if self.files.is_empty() {
            let _ = writeln!(
                out,
                "STOPPED LOOKING: the sweep examined no .rs file at all, so this is not a \
                 compliant tree, it is a category that has stopped matching"
            );
            return out;
        }

        for file in &self.files {
            match &file.outcome {
                Outcome::Measured(counts) => {
                    let _ = writeln!(
                        out,
                        "FILE {} prose={} code={} ratio={}",
                        file.path,
                        counts.prose,
                        counts.code,
                        format_tenths(counts.tenths())
                    );
                }
                Outcome::Unreadable(why) => {
                    let _ = writeln!(out, "UNREADABLE {} {}", file.path, why);
                }
                Outcome::Untokenizable(error) => {
                    let _ = writeln!(out, "UNTOKENIZABLE {} {}", file.path, error);
                }
            }
        }

        let mut generated = 0usize;
        let mut headers = 0usize;
        let mut small = 0usize;
        for file in &self.files {
            let Outcome::Measured(counts) = &file.outcome else {
                continue;
            };
            if let Some(marker) = &counts.generated_marker {
                generated += 1;
                let _ = writeln!(
                    out,
                    "EXCLUDED {} generated-file marker {:?}, so the ceiling does not apply to it",
                    file.path, marker
                );
            }
            if let Some(opener) = &counts.header_opener {
                headers += 1;
                let _ = writeln!(
                    out,
                    "LICENCE-HEADER {} {} line(s) not counted, opening {:?}",
                    file.path, counts.header_lines, opener
                );
            }
            if counts.generated_marker.is_none() && counts.counted() < cfg.minimum_counted_lines {
                small += 1;
                let _ = writeln!(
                    out,
                    "BELOW-MINIMUM {} counted={} under the {}-line minimum, so the ceiling does \
                     not apply to it",
                    file.path,
                    counts.counted(),
                    cfg.minimum_counted_lines
                );
            }
        }

        for (path, counts) in self.measured() {
            if counts.counted() < cfg.minimum_counted_lines {
                continue;
            }
            let ratio = counts.tenths();
            if ratio > cfg.ceiling_percent * 10 {
                let _ = writeln!(
                    out,
                    "OVER {} ratio={} is over the {}% ceiling",
                    path,
                    format_tenths(ratio),
                    cfg.ceiling_percent
                );
            } else if ratio > cfg.warn_percent * 10 {
                let _ = writeln!(
                    out,
                    "WARN {} ratio={} is in the warn band, over {}% and not over {}%",
                    path,
                    format_tenths(ratio),
                    cfg.warn_percent,
                    cfg.ceiling_percent
                );
            }
        }

        let (prose, code) = self.aggregate();
        let _ = writeln!(
            out,
            "aggregate prose={} code={} ratio={} over {} measured file(s), and it gates nothing",
            prose,
            code,
            format_tenths(tenths(prose, code)),
            self.measured().count()
        );
        let _ = writeln!(out, "excluded as generated: {} file(s)", generated);
        let _ = writeln!(out, "carrying a licence header: {} file(s)", headers);
        let _ = writeln!(
            out,
            "below the {}-line minimum and excluded from the ceiling test: {} file(s)",
            cfg.minimum_counted_lines, small
        );
        out
    }
}

fn read_source(path: &Path) -> Result<String, String> {
    let bytes = fs::read(path).map_err(|error| error.to_string())?;
    String::from_utf8(bytes).map_err(|_| "not valid UTF-8".to_string())
}

// What git says is tracked, which is what the criteria are written about. The
// demonstration trees are pruned here for the same reason the pinning scanner
// prunes its own.
pub fn tracked_rust_files(root: &Path) -> Result<Vec<String>, Missing> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["ls-files", "-z", "--", "*.rs"])
        .output()
        .map_err(|error| Missing {
            criterion: "the comment-density ceiling over every tracked .rs file".to_string(),
            prerequisite: format!("git, to list what is tracked; running it failed ({error})"),
            how: "install git and run this from inside the chorus working tree".to_string(),
        })?;
    if !output.status.success() {
        return Err(Missing {
            criterion: "the comment-density ceiling over every tracked .rs file".to_string(),
            prerequisite: format!(
                "a git working tree at {}; git ls-files exited {}",
                root.display(),
                output.status
            ),
            how: "run this from inside the chorus working tree".to_string(),
        });
    }
    let listing = String::from_utf8_lossy(&output.stdout);
    Ok(listing
        .split('\0')
        .filter(|path| !path.is_empty() && !path.starts_with(DEMONSTRATIONS))
        .map(|path| path.to_string())
        .collect())
}

// A demonstration tree is walked rather than asked about, so a fixture can be
// built in a temporary directory that no repository tracks.
pub fn walked_rust_files(dir: &Path) -> io::Result<Vec<String>> {
    let mut found = Vec::new();
    walk(dir, dir, &mut found)?;
    found.sort();
    Ok(found)
}

fn walk(root: &Path, dir: &Path, found: &mut Vec<String>) -> io::Result<()> {
    let mut entries: Vec<_> = fs::read_dir(dir)?.collect::<io::Result<Vec<_>>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        // Never follow into a symlink and never resolve one: a dangling .rs
        // symlink is a file that cannot be read, and skipping it would be the
        // silent omission the unreadable criterion forbids.
        let kind = entry.file_type()?;
        if kind.is_dir() {
            walk(root, &path, found)?;
        } else if path.extension().map(|ext| ext == "rs").unwrap_or(false) {
            let relative = path.strip_prefix(root).unwrap_or(&path);
            found.push(relative.to_string_lossy().replace('\\', "/"));
        }
    }
    Ok(())
}
