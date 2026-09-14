# The control page's committed identity

The umbrella's `.sdd/conventions/interface-craft.md` clause C1 asks every repo to
commit a design record naming its display face, text face, accent, radius
signature and shadow signature, each with one sentence saying why that value.
Clause C2 names the values a surface is not built from, and grants an exception
to a repo that names one here with its reason. chorus ships one surface: the
control page the server hands out on its control listener, whose sources are
`crates/server/src/ui/`. This file is that record.

It sits beside `docs/styling-conventions-record.md` and grades a different
convention. The styling record maps the ten styling clauses onto the checks that
prove them; this one names the five identity values and the C2 defaults this
surface is allowed to carry. Neither reads the other.

It is not prose that can drift. `tools/ui/interface-craft-scan.js` reads the two
tables below on every `make verify-interface-craft` and exits non-zero, naming
what it found, when an entry is missing, when a declared value and the token file
disagree, when an exception carries no reason, and when an exception is for a
default no source carries any more.

## How a machine reads this file

Two tables, each under the heading above it, each with a fixed set of columns.

The `declares` column of the identity table takes one of two forms and nothing
else:

- a list of `` `--token` = `value` `` pairs separated by `; `, where every name
  is declared in `crates/server/src/ui/tokens.css` and every value is that
  file's declaration of it, character for character once runs of whitespace are
  collapsed. A token declared once per theme is written once per theme, because
  agreement is checked against every declaration and not against the first one.
- the word `absent`, then `; kept absent by`, then a path in backticks, then the
  word `rule`, then a name in backticks. That is for a value this surface does
  not have. The path has to be a file in this repository and the rule name has to
  appear in it, so a value declared absent names the committed check that keeps
  it absent rather than resting on nobody having added one. The shadow signature
  row below is the shape.

The `why` column of both tables is one sentence, and the scan refuses an empty
one: an exception without a stated reason is treated as no exception at all, and
every default it was covering is reported.

## The identity

| entry | declares | why |
|---|---|---|
| display face | `--face-prose` = `var(--font-ui)`; `--font-ui` = `-apple-system, "Segoe UI", Roboto, "Helvetica Neue", Arial, sans-serif`; `--wordmark-size` = `var(--text-20)`; `--heading-size` = `var(--text-17)` | chorus declares no separate display face: the wordmark and every heading are set in the same UI face as the prose, one and two steps up the type scale, because a page this dense reads better in one voice than with a second family competing for attention inside a 44 pixel bar. |
| text face | `--face-prose` = `var(--font-ui)`; `--face-figure` = `var(--font-mono)`; `--font-ui` = `-apple-system, "Segoe UI", Roboto, "Helvetica Neue", Arial, sans-serif`; `--font-mono` = `ui-monospace, SFMono-Regular, Menlo, monospace` | Prose, headings and controls are set in the UI face and anything whose column alignment carries meaning is set in the fixed-advance face, which is styling S5 applied as written: the UI face is the one the operating system dresses its own interface in, so the page matches the machine it is read on, and the figure face holds equal digits at equal width so a column of delay figures can be read down rather than across. |
| accent | `--accent` = `var(--blue-600)`; `--accent` = `var(--blue-300)`; `--blue-600` = `#0a5387`; `--blue-300` = `#7ab8f5`; `--wordmark-ink` = `var(--accent)` | One hue outside the four state roles, per styling S10, and it is a deep navy-leaning blue picked by hand for each theme: `#0a5387` measures 8.08:1 on the light panel and `#7ab8f5` measures 7.73:1 on the dark one off the painted pixels, and blue is already the hue the wordmark, every link and the fill of a pressed control spend, so a second hue would have to displace it rather than join it. |
| radius signature | `--surface-radius` = `var(--radius-12)`; `--control-radius` = `var(--radius-8)`; `--slider-track-radius` = `var(--radius-4)`; `--radius-12` = `12px`; `--radius-8` = `8px`; `--radius-4` = `4px` | Three steps rather than one, each bound to the size of the box its corner belongs to: 12px on a zone panel, 8px on a control and 4px on the slider track, so a corner says how large the thing it turns is instead of one rounding being stamped over every box on the page. |
| shadow signature | absent; kept absent by `tools/ui/styling-scan.js` rule `border-and-surface-separation` | This surface draws no shadow at all and has no depth channel to budget: regions are told apart by a border token and a surface token, which is styling S8, and the rule named here refuses a `box-shadow` or a `text-shadow` anywhere in these sources, so the absence is held by a check rather than by the absence of anyone adding one. |

## The C2 exceptions

Clause C2's last sentence: "A repo wanting one of these names it in C1 with its
reason and it is allowed." These five are the defaults this surface carries, each
with the reason it carries it. The scan refuses a sixth that no source needs, so
a permission that stops being used is withdrawn rather than left standing.

| blocklist entry | why |
|---|---|
| `Roboto` | The UI stack names Android's interface face in the position an Android system reads it, and the entry is on C2's list because a repo declaring Roboto as ITS face is taking the training-data default; chorus declares no face of its own and hands the choice to the operating system, which is what styling S5 asks of prose and controls. |
| `Helvetica` | `"Helvetica Neue"` is the macOS fallback standing behind `-apple-system` for a system old enough not to resolve that keyword, and it is named for the same reason Roboto is: it is a platform's own face in that platform's position and not a face chorus chose for itself. |
| `Arial` | Arial is the last named family before the generic keyword, for a Windows system that resolves neither `-apple-system` nor `"Segoe UI"`, and naming it is what keeps the stack from falling all the way to whatever `sans-serif` happens to be pointed at on that machine. |
| `a bare system stack alone` | Both faces this page declares are system stacks with no repo-chosen family standing ahead of them, and that is the choice rather than the absence of one: styling S5 says prose and controls ARE system sans, so a bespoke face here would be a file to download, a layout shift to absorb and a second thing to keep legible at 13px, and `make verify-ui` measures which shape actually arrived rather than trusting the stack. |
| `blue-600` | `--blue-600` is this repository's own primitive at `#0a5387` and not Tailwind's `blue-600` at `#2563eb`, and the name is a position on chorus's own lightness ladder where every primitive is labelled by how light it is; the identifier is on C2's list because a page shipping Tailwind's value ships Tailwind's look, and no check can tell the two apart by the name alone, so the record is where this one says which it is. |
