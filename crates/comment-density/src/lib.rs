pub mod count;
pub mod demos;
pub mod lex;
pub mod record;
pub mod sweep;

// Set from this repository's own measured baseline. docs/comment-density-record.md
// carries the distribution they were read off and the sentence that says which
// one; the gate refuses if the record and these disagree.
pub const CEILING_PERCENT: u32 = 45;
pub const WARN_PERCENT: u32 = 35;
pub const MINIMUM_COUNTED_LINES: usize = 30;

// Distinct per failure mode, so a red build says which regression broke rather
// than "the gate". The Makefile target carries the same table.
pub const EXIT_OK: i32 = 0;
pub const EXIT_OVER_CEILING: i32 = 2;
pub const EXIT_RECORD_DISAGREES: i32 = 3;
pub const EXIT_UNTOKENIZABLE: i32 = 4;
pub const EXIT_UNREADABLE: i32 = 5;
pub const EXIT_EMPTY_SWEEP: i32 = 6;
pub const EXIT_DEMONSTRATION: i32 = 7;
pub const EXIT_MISSING_CAPABILITY: i32 = 8;

pub const EXIT_CODES: &[(i32, &str)] = &[
    (EXIT_OK, "the tree is under the ceiling and every demonstration produced what it demonstrates"),
    (EXIT_OVER_CEILING, "a measured file is over the ceiling the record declares"),
    (EXIT_RECORD_DISAGREES, "docs/comment-density-record.md and this gate disagree"),
    (EXIT_UNTOKENIZABLE, "a tracked .rs file could not be tokenized to completion"),
    (EXIT_UNREADABLE, "a tracked .rs file could not be read"),
    (EXIT_EMPTY_SWEEP, "the sweep matched no tracked .rs file at all"),
    (EXIT_DEMONSTRATION, "a committed demonstration did not produce what it demonstrates"),
    (EXIT_MISSING_CAPABILITY, "a capability this check needs is missing"),
];

// tools/lib.sh's refusal shape, in the language the check is written in: exit
// non-zero, name the prerequisite and the criterion it blocks, and print
// nothing that reads as passed, skipped-green or satisfied.
#[derive(Debug, Clone)]
pub struct Missing {
    pub criterion: String,
    pub prerequisite: String,
    pub how: String,
}

impl Missing {
    pub fn render(&self) -> String {
        format!(
            "MISSING PREREQUISITE\n  criterion:    {}\n  prerequisite: {}\n  how to get it: {}\n  \
             this check is NOT passed, NOT skipped-green and NOT satisfied.\n",
            self.criterion, self.prerequisite, self.how
        )
    }
}

pub fn default_config(
    directive_prefixes: Vec<String>,
    generated_markers: Vec<String>,
) -> count::Config {
    count::Config {
        ceiling_percent: CEILING_PERCENT,
        warn_percent: WARN_PERCENT,
        minimum_counted_lines: MINIMUM_COUNTED_LINES,
        directive_prefixes,
        generated_markers,
    }
}
