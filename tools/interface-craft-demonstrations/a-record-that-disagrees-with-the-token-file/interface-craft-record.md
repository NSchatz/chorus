# A record whose accent is not the accent the token file declares

base/interface-craft-record.md with one value changed: the accent row says
`--teal-600` is `#0a6b6a`, and `ui/tokens.css` beside it declares `#0a6b6b`. One
hexadecimal digit.

C1 is graded as "the record exists, names all five, and the token file matches
it". A record that has drifted one digit away from the file it describes is the
thing that grading is for: it reads as authoritative and it is wrong, and
nothing but a check re-derives it.

## The identity

| entry | declares | why |
|---|---|---|
| display face | `--face-prose` = `var(--font-ui)`; `--font-ui` = `"Chorus Display", -apple-system, sans-serif` | A repo-chosen face stands at the head of the stack, so this tree is not a bare system stack alone and the demonstration beside it that IS one differs in exactly that. |
| text face | `--face-prose` = `var(--font-ui)`; `--face-figure` = `var(--font-mono)`; `--font-mono` = `"Chorus Mono", ui-monospace, monospace` | Prose takes the display face and anything whose columns carry meaning takes the fixed-advance one, which is the two-role split the real record describes. |
| accent | `--accent` = `var(--teal-600)`; `--teal-600` = `#0a6b6a` | One hue, named for this tree rather than for a framework, so that the demonstrations which add an `indigo-500` or a `blue-600` beside it differ in exactly that. |
| radius signature | `--surface-radius` = `var(--radius-12)`; `--radius-12` = `12px` | One declared corner, spent by the only surface this tree paints, because a base tree exists to hold the rules rather than to be a page. |
| shadow signature | absent; kept absent by `tools/ui/styling-scan.js` rule `border-and-surface-separation` | Nothing here is raised, and the entry names the committed rule that refuses a box shadow so that the absent form of a declaration is exercised by the tree that has to pass. |

## The C2 exceptions

| blocklist entry | why |
|---|---|
