# 0022: the control page's identity is committed, and five C2 defaults are exceptions

- Status: decided
- Recorded by: S0124-chorus-interface-craft-design-record, against the umbrella's
  `.sdd/conventions/interface-craft.md` clauses C1 and C2
- Implemented in: `docs/interface-craft-record.md`,
  `tools/ui/interface-craft-scan.js`, `tools/interface-craft-check.sh`,
  `tools/interface-craft-demonstrations/`

## The question

C1 asks a repo to commit a design record naming its display face, text face,
accent, radius signature and shadow signature, each with one sentence saying why
that value. C2 names the values a surface is not built from and ends: "A repo
wanting one of these names it in C1 with its reason and it is allowed."

chorus had neither. `crates/server/src/ui/tokens.css` declares every value C1
asks about, and `docs/styling-conventions-record.md` says which styling clause
each check proves, but nothing said WHICH identity those tokens express or why,
so C1 was unmet and C2 had no declared exception set to grade against.

The awkward part is what a first run of a C2 check finds. Read off the tokens
this page already ships:

- `--font-ui` is `-apple-system, "Segoe UI", Roboto, "Helvetica Neue", Arial,
  sans-serif`. That names three of C2's five refused faces, and every family in
  it is a system face or a generic keyword, which is C2's "a bare system stack
  alone". Four entries in one declaration.
- `--accent` binds the primitive `--blue-600`, and `blue-600` is the identifier
  C2 lists as Tailwind's.

So a C2 check goes red on day one, on five entries, over a page nobody thinks is
generic.

## The decision

Those five are exceptions, written into the record with their reasons, and no
token value moves.

The reasoning is that C2 is a check on a CHOICE and not on a string. The list
exists because an unstated choice gets filled with the training-data average, and
the average interface declares Inter or Roboto as its own face and paints
Tailwind's `blue-600`. Neither is what this page does:

- The faces are not chorus's faces. The stack hands the choice to the operating
  system and names each platform's own interface face in the position that
  platform reads it. The umbrella's styling clause S5 says in as many words that
  prose and controls ARE system sans, so a bespoke face here would be a
  deliberate departure from a convention this repo is held to, and the four face
  entries reconcile C2 with S5 rather than conceding a defect to C2.
- `--blue-600` is this repository's own primitive at `#0a5387`, on a ladder where
  every primitive is named for how light it is. Tailwind's `blue-600` is
  `#2563eb`. The two share a name and nothing else, and the record is the only
  place that difference can be stated, because no check can tell them apart by
  the name alone.

The alternative was to rename `--blue-600` and to put a repo-chosen face at the
head of the stack, which would turn the check green by changing the surface. That
is the wrong way round twice: it would spend a font download, a layout shift and
a second legibility problem at 13px to satisfy a list, and it would rename a
primitive so that a string match stops firing, which is the shape of a change
that makes a check quieter without making a page better.

## Why the check is mechanical, and what it is written in

C2's own `*Graded:*` line asks for "a mechanical check over the stylesheet and
template sources", and says why: a prohibition a machine holds does not decay
with session depth, where one a model holds falls to 33% by turn 16. An exception
is only worth granting if a check is standing behind it, and a record nobody
verifies is a record that drifts away from the tokens it describes.

The scan is written in JavaScript, beside `tools/ui/styling-scan.js`, and run by
`tools/interface-craft-check.sh` the way `tools/styling-check.sh` runs that one.
The fitness argument is that this is the same job over the same four files:
reading CSS declarations, resolving `var()` chains through the token file and
turning a colour into a hue angle. `tools/ui/tokens.js` already does all three
and is already held to the shape of `crates/server/src/ui/tokens.css`, so the new
scan reuses it and cannot come to a different conclusion than the styling scan
does about what a token is declared as. Writing it a second time in another
language would be a second parser of the same file, and two parsers of one format
disagree eventually. What the interpreter happens to be on any machine is not a
reason either way (working agreement 8): an absent one is installed, and
`tools/lib.sh`'s `require_node` guard is what makes an absent one a named refusal
rather than a green build.

## What holds it up

- `make verify-interface-craft` reads the record, resolves
  `crates/server/src/ui/` rather than a written-down list of four files, sweeps
  every source against all fifteen blocklist entries, and prints what it read and
  what it tested so a reader is not inferring the sweep from a zero exit.
- `tools/interface-craft-demonstrations/` holds one committed tree per blocklist
  entry, each required to go red naming that entry and no other, a tree that
  carries none of them and is required to pass, and one tree for each remaining
  way this gate can fail. The run fails if a committed tree was not exercised.
- Every one of those failures has its own exit code, listed above the target in
  the `Makefile` and in the check's `--help`, so a red build says which kind of
  fault it was.
