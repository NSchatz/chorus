# 0020: the comment density baseline

- Status: decided
- Recorded by: spec S0113-chorus-comment-prose
- Implemented in: `crates/comment-density`, `make verify-comment-density`,
  `make verify-comment-density-suite`, `docs/comment-density-record.md`,
  `tools/comment-density-demonstrations/`

## The question

0002 holds `cargo fmt --check` and `cargo clippy` back from being gates because
"no formatting or lint baseline has been agreed for this repository, and a gate
nobody has agreed on only teaches people to ignore red. They arrive with the
first entry that agrees the baseline."

This is that entry, for one property and no other: how much of a Rust source
file may be comment prose. It agrees nothing about formatting, nothing about
clippy, and nothing about how a comment is worded.

## What was measured, before anything was decided

Every tracked `.rs` file in the repository, counted from its Rust token stream.
117 files, 7506 lines of prose over 30042 lines of code, 19.9% in aggregate.
`docs/comment-density-record.md` carries the whole distribution, file by file,
and the counting stance in full.

The distribution splits once, and cleanly:

| where | files | ratio |
|---|---|---|
| the densest file measured, `crates/audio-path/src/lib.rs` | 1 | 86.6% |
| the crate roots above the gap, each a `crates/*/src/lib.rs` | 8 | 57.5% to 86.6% |
| the densest file that carries code, `crates/client-linux/src/delaylog.rs` | 1 | 44.1% |
| every file that carries code | 109 | 44.1% and below |

There is nothing between 44.1% and 57.5%. The eight files above the gap are
crate roots: their prose is crate-level design documentation and their code is
`pub mod` and `pub use` lines, so the ratio is measuring the shape of a crate
root rather than whether anybody narrated anything.

## Decision

- **Ceiling 90%.** The first round number above the densest file measured. It is
  a no-regression ceiling and not a target. A file may not become almost pure
  narration; nothing in the tree today is asked to change.
- **Warn band 45%.** The first round number above the densest file that carries
  code. A file that drifts up into crate-root territory is named on every run
  and does not fail the build.
- **Minimum 30 counted lines.** Below it a file is excluded from the ceiling
  test and still reported, so nobody hides a file by shrinking it. Two files are
  below it today.
- **The record is the ratchet.** The gate reads every row of
  `docs/comment-density-record.md` against the tree, so any change to any file's
  prose or code line count is a red build until the record is regenerated with
  `make comment-density-record` and committed. That regeneration cannot make a
  red ceiling green: the ceiling is measured against the tree and never against
  the record. The cost is a table that moves whenever a comment does, and that
  cost is the point: the diff of that table is where a comment that crept in
  becomes visible to a reviewer.

## Why nothing was trimmed

The spec that produced this entry asked for the files over the intended ceiling
to be trimmed. There are none, and the reason is in the distribution above
rather than in the choice of ceiling.

Any ceiling tight enough to catch a file in this tree catches the eight crate
roots and nothing else. Trimming those means deleting crate-level documentation:
why the audio path is an enumeration and not a runtime property, why the control
catalog is a second catalog beside the audio wire rather than inside it, which
parts of RFC 6762 the discovery crate deliberately does not implement. Every one
of those says something the code cannot, which is the test this repository
already applies to a comment.

The counting stance this gate uses counts doc comments as prose deliberately, so
that relabelling `//` as `///` is not a way past the ceiling, and it says in the
same breath that the ceiling is therefore set from a baseline measured with them
counted "so a well documented crate is not penalised for being one". Holding the
crate roots to a tighter number would be exactly that penalty.

So the measurement is the finding: this repository's Rust is documented, not
narrated. A search of every `.rs` file for the shapes that usually justify a
trim, comments narrating a change's history and commented-out code, returns
nothing. What remains for a future item is the languages this measurement
excludes, and the trim of any file this ceiling puts over it later.

## What is measured, and what is not

Rust only, and every tracked `.rs` file. The other languages in this tree each
need their own lexer, and a second half-right lexer is the failure this whole
exercise exists to avoid: the umbrella's own first pass at this check matched
quote characters, read the closing quote of a multi-line string as a comment
opening one, and scored five mostly-code files between 64% and 85% prose. The
only reason those files were not gutted is that somebody measured first.

Not measured here, each its own item with its own lexer and its own measured
baseline, in the order their prose density makes them worth doing:

1. the bash entry points under `tools/` and the two Makefiles, which carry the
   densest comment blocks in the tree;
2. `.github/workflows/ci.yml`, whose steps are majority prose;
3. the C11 endpoint under `firmware/`, which needs a C lexer with its own string
   forms;
4. the JavaScript under `tools/ui/`.

`tools/comment-density-demonstrations/` is pruned from the sweep. Those trees
are over the ceiling by construction and counting them would leave the
repository permanently red for the reason the gate exists to prevent.

## What the gate gates, and what it does not

It gates: a measured file's ratio against the ceiling; the record's declared
ceiling, warn band and minimum against the numbers the gate compiles in; every
record row against the tree; a row whose two code-line counts differ, which is a
trim that moved a code token; a row naming a path the gate does not measure; a
`.rs` file that cannot be read or cannot be tokenized, each named rather than
counted as zero; a sweep that matched nothing at all; and each of the six
committed demonstrations, which must produce the failure it demonstrates.

It does not gate: formatting, clippy, how a comment is worded, the repository
aggregate, any file below the minimum, any generated file, or any language other
than Rust.

## Dependencies

None. The counter is `std` only, like every other crate in this workspace, so
`Cargo.lock` gains a workspace member and no external package. That is what lets
`make verify-comment-density` inherit `make verify-pinning`'s property of
needing no network: it resolves nothing, opens no socket and asks no registry
anything. BRIEF.md 3.2's preference for building the small and instructive over
vendoring it is the same reason `crates/protocol` owns its own PRNG.

## Consequences

- `make verify` runs one more check and CI names one more step, both of which
  need no device, no privilege and no network.
- A change that adds or removes a comment reddens the build until
  `make comment-density-record` is run and the result committed.
- A new `.rs` file needs a row in the record. Its before column reads `-`, which
  is the honest answer for a file that did not exist when the baseline was
  measured.
- A change that adds or removes a line of CODE needs more than that regeneration:
  the two code-line counts in that file's row are the rule that a trim moves no
  code token, so a code change makes them disagree by construction and
  `make comment-density-record` carries the old before column straight into the
  disagreement. Such a change re-takes the baseline with
  `cargo run -p chorus-comment-density -- record --baseline`, which is a
  deliberate statement that the tree being compared against has moved. It cannot
  make a red ceiling green: the ceiling is measured against the tree and never
  against the record.

## Revisit when

A file is genuinely trimmed, which is what re-ratchets the ceiling downward; or
a second language is measured, which needs its own entry and its own baseline
rather than an amendment to this one.
