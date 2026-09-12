// Lines attributed from the token stream, and the exclusions the stance
// declares. Nothing here reads a line and guesses what it is; every line gets
// its character from the tokens that fall on it.

use std::collections::BTreeSet;

use crate::lex::{scan, Kind, LexError, Lexeme};

#[derive(Debug, Clone)]
pub struct Config {
    pub ceiling_percent: u32,
    pub warn_percent: u32,
    pub minimum_counted_lines: usize,
    pub directive_prefixes: Vec<String>,
    pub generated_markers: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Counts {
    pub prose: usize,
    pub code: usize,
    pub header_lines: usize,
    pub header_opener: Option<String>,
    pub generated_marker: Option<String>,
}

impl Counts {
    pub fn counted(&self) -> usize {
        self.prose + self.code
    }

    pub fn tenths(&self) -> u32 {
        tenths(self.prose, self.code)
    }
}

// A ratio in tenths of a percent, integer throughout: a gate that compared
// floats would drift against a record that stores decimal text.
pub fn tenths(prose: usize, code: usize) -> u32 {
    let counted = prose + code;
    if counted == 0 {
        return 0;
    }
    ((prose * 1000) / counted) as u32
}

pub fn format_tenths(value: u32) -> String {
    format!("{}.{}%", value / 10, value % 10)
}

pub fn count(src: &str, cfg: &Config) -> Result<Counts, LexError> {
    let lexemes = scan(src)?;
    let starts = line_starts(src);
    let blank = blank_lines(src);

    let mut comment_lines: BTreeSet<usize> = BTreeSet::new();
    let mut code_lines: BTreeSet<usize> = BTreeSet::new();

    for lexeme in &lexemes {
        let first = line_at(&starts, lexeme.start);
        let last = line_at(&starts, lexeme.end.saturating_sub(1));
        match lexeme.kind {
            Kind::Code => {
                for line in first..=last {
                    code_lines.insert(line);
                }
            }
            Kind::Comment => {
                let text = comment_text(&src[lexeme.start..lexeme.end]);
                let target = if is_directive(&text, &cfg.directive_prefixes) {
                    &mut code_lines
                } else {
                    &mut comment_lines
                };
                for line in first..=last {
                    target.insert(line);
                }
            }
        }
    }

    let block = leading_block(&lexemes, src, &starts);
    let mut generated_marker = None;
    let mut header_opener = None;
    let mut header: BTreeSet<usize> = BTreeSet::new();
    if !block.is_empty() {
        let mut text: Vec<String> = Vec::new();
        for lexeme in &block {
            text.extend(comment_text(&src[lexeme.start..lexeme.end]));
        }
        generated_marker = marker_in(&text, &cfg.generated_markers);
        if let Some(opener) = licence_opener(&text) {
            header_opener = Some(opener);
            for lexeme in &block {
                let first = line_at(&starts, lexeme.start);
                let last = line_at(&starts, lexeme.end.saturating_sub(1));
                for line in first..=last {
                    header.insert(line);
                }
            }
        }
    }

    // A licence header is an obligation rather than prose an author chose, so
    // its lines leave both counts. A line it shares with code is code, which is
    // why the code side is subtracted first.
    for line in &code_lines {
        header.remove(line);
    }

    let prose = comment_lines
        .iter()
        .filter(|line| !code_lines.contains(line) && !header.contains(line) && !blank.contains(line))
        .count();
    let code = code_lines.iter().filter(|line| !blank.contains(line)).count();

    Ok(Counts {
        prose,
        code,
        header_lines: header.len(),
        header_opener,
        generated_marker,
    })
}

// The leading comment block: the run of comments a file opens with and no more
// than that. It ends at the first code token, at the first blank line between
// two comments, and at the first change of comment form. Both exclusions below
// are scoped to it, so one licence line or one generated marker excuses the
// block it opens and never a documentation block written under it: a rule that
// ran to the first code token would make prepending "SPDX-License-Identifier"
// the way past the ceiling that counting doc comments as prose exists to close.
fn leading_block<'a>(lexemes: &'a [Lexeme], src: &str, starts: &[usize]) -> Vec<&'a Lexeme> {
    let mut block: Vec<&Lexeme> = Vec::new();
    let mut opened: Option<Form> = None;
    let mut previous_last = 0usize;
    for lexeme in lexemes {
        if lexeme.kind != Kind::Comment {
            break;
        }
        let form = comment_form(&src[lexeme.start..lexeme.end]);
        let first = line_at(starts, lexeme.start);
        if let Some(opened) = opened {
            if form != opened || first > previous_last + 1 {
                break;
            }
        }
        opened = Some(form);
        previous_last = line_at(starts, lexeme.end.saturating_sub(1));
        block.push(lexeme);
    }
    block
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Form {
    Line,
    OuterLineDoc,
    InnerLineDoc,
    Block,
    OuterBlockDoc,
    InnerBlockDoc,
}

// Which of Rust's six comment forms a lexeme is written in. `////` is an
// ordinary line comment rather than a run of doc slashes, and `/**/` is an empty
// block comment rather than an outer block doc, so each doc form has to look at
// what follows its marker.
fn comment_form(raw: &str) -> Form {
    if let Some(rest) = raw.strip_prefix("//") {
        if rest.starts_with('!') {
            return Form::InnerLineDoc;
        }
        if rest.starts_with('/') && !rest.starts_with("//") {
            return Form::OuterLineDoc;
        }
        return Form::Line;
    }
    let rest = raw.strip_prefix("/*").unwrap_or(raw);
    if rest.starts_with('!') {
        return Form::InnerBlockDoc;
    }
    if rest.starts_with('*') && !rest.starts_with("*/") {
        return Form::OuterBlockDoc;
    }
    Form::Block
}

fn line_starts(src: &str) -> Vec<usize> {
    let mut starts = vec![0usize];
    for (offset, byte) in src.bytes().enumerate() {
        if byte == b'\n' {
            starts.push(offset + 1);
        }
    }
    starts
}

fn line_at(starts: &[usize], offset: usize) -> usize {
    match starts.binary_search(&offset) {
        Ok(index) => index,
        Err(index) => index - 1,
    }
}

fn blank_lines(src: &str) -> BTreeSet<usize> {
    let mut blank = BTreeSet::new();
    for (index, line) in src.split('\n').enumerate() {
        if line.trim().is_empty() {
            blank.insert(index);
        }
    }
    blank
}

// A comment's text, one entry per line, with the markers that make it a comment
// taken off. Doc comments in all four forms reduce to the same thing here,
// because all four are prose and only the text decides the rest.
pub fn comment_text(raw: &str) -> Vec<String> {
    if let Some(rest) = raw.strip_prefix("//") {
        let rest = rest.trim_start_matches('/');
        let rest = rest.strip_prefix('!').unwrap_or(rest);
        return vec![rest.trim().to_string()];
    }
    let rest = raw.strip_prefix("/*").unwrap_or(raw);
    let rest = rest.strip_prefix('*').or_else(|| rest.strip_prefix('!')).unwrap_or(rest);
    let rest = rest.strip_suffix("*/").unwrap_or(rest);
    rest.split('\n')
        .map(|line| {
            let line = line.trim();
            let line = if line.len() > 1 && line.starts_with('*') && !line.starts_with("*/") {
                line[1..].trim()
            } else {
                line
            };
            line.to_string()
        })
        .collect()
}

fn first_text(text: &[String]) -> Option<&str> {
    text.iter().map(|line| line.as_str()).find(|line| !line.is_empty())
}

fn is_directive(text: &[String], prefixes: &[String]) -> bool {
    let Some(first) = first_text(text) else {
        return false;
    };
    prefixes.iter().any(|prefix| first.starts_with(prefix.as_str()))
}

fn marker_in(text: &[String], markers: &[String]) -> Option<String> {
    for line in text {
        for marker in markers {
            if line.contains(marker.as_str()) {
                return Some(marker.clone());
            }
        }
    }
    None
}

fn licence_opener(text: &[String]) -> Option<String> {
    let first = first_text(text)?;
    let lowered = first.to_ascii_lowercase();
    if first.starts_with("SPDX-License-Identifier")
        || lowered.starts_with("copyright")
        || lowered.starts_with("(c)")
    {
        return Some(first.to_string());
    }
    None
}
