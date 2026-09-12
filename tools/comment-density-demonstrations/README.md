# comment-density demonstrations

Six small trees: five wrong on purpose and one right on purpose.
`make verify-comment-density` scans each one and requires it to produce the
failure it demonstrates, with the exit code the table gives and the report
naming what the table says it must name.

They exist because a check that has never been seen to fail is a formality
rather than evidence. This gate needs them more than most: a run over a
compliant tree exits zero whether or not the counter reads a raw string
correctly, so nothing the repository sweep does can tell a working counter from
a broken one.

| directory | shape | wanted exit | the report must name |
|---|---|---|---|
| `a-file-over-the-ceiling` | a file whose own comments put it over the ceiling | 2 | `OVER narrated.rs` |
| `comment-like-text-in-strings` | comment-like text inside strings, raw strings, byte strings and characters | 2 | `FILE strings.rs prose=0 code=`, `OVER narrated.rs` |
| `a-file-that-cannot-be-tokenized` | a block comment that runs off the end of the file | 4 | `UNTOKENIZABLE unterminated.rs byte ` |
| `a-file-that-cannot-be-read` | a `.rs` path that cannot be opened | 5 | `UNREADABLE gone.rs` |
| `an-empty-sweep` | a tree in which the category matches nothing at all | 6 | `STOPPED LOOKING` |
| `a-tree-that-passes` | a documented file in the warn band beside a file under the minimum | 0 | `FILE documented.rs`, `BELOW-MINIMUM tiny.rs`, `WARN documented.rs` |

`comment-like-text-in-strings` is the one that carries the weight. It requires
`strings.rs` to be reported at `prose=0` while its neighbour goes over the
ceiling. A counter that went back to matching quote characters would put both
files over the ceiling, the `prose=0` line would be gone, and this tree would
stop producing what it demonstrates. That is the umbrella's original failure,
which scored five mostly-code files between 64% and 85% prose, held here as a
committed counterexample rather than as a warning in a comment.

`a-tree-that-passes` is why five red trees are not enough on their own. A gate
that refused every commented file and a gate that refuses only the narrated ones
leave identical evidence against the other five, so a file carrying real
documentation is committed too and has to pass. It sits in the warn band, so the
same tree shows that a warn is named without failing the build, and it carries a
file under the minimum, so the same tree shows that a small file is excluded from
the ceiling and still reported.

`gone.rs` is a directory rather than a file; its own README says why.

The sweep prunes this directory when it scans the repository. It has to: these
trees are over the ceiling by construction, and counting them against chorus
would leave the repository permanently red for the exact reason the gate exists
to prevent. The gate checks that every directory here is exercised, so one added
and forgotten is a failure rather than a silence.

Nothing here is built, installed or run. `cargo` never sees these files: they are
under `tools/` and belong to no crate. They are read.
