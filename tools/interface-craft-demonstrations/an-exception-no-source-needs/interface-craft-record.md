# A record that excepts an entry no source carries

base/interface-craft-record.md with one exception row added, carrying a perfectly
good reason, over a set of sources that carries no Roboto anywhere. A permission
that has stopped being needed is withdrawn rather than left standing over the
surface, so this is refused.

## The identity

| entry | declares | why |
|---|---|---|
| display face | `--face-prose` = `var(--font-ui)`; `--font-ui` = `"Chorus Display", -apple-system, sans-serif` | A repo-chosen face stands at the head of the stack, so this tree is not a bare system stack alone and the demonstration beside it that IS one differs in exactly that. |
| text face | `--face-prose` = `var(--font-ui)`; `--face-figure` = `var(--font-mono)`; `--font-mono` = `"Chorus Mono", ui-monospace, monospace` | Prose takes the display face and anything whose columns carry meaning takes the fixed-advance one, which is the two-role split the real record describes. |
| accent | `--accent` = `var(--teal-600)`; `--teal-600` = `#0a6b6b` | One hue, named for this tree rather than for a framework, so that the demonstrations which add an `indigo-500` or a `blue-600` beside it differ in exactly that. |
| radius signature | `--surface-radius` = `var(--radius-12)`; `--radius-12` = `12px` | One declared corner, spent by the only surface this tree paints, because a base tree exists to hold the rules rather than to be a page. |
| shadow signature | absent; kept absent by `tools/ui/styling-scan.js` rule `border-and-surface-separation` | Nothing here is raised, and the entry names the committed rule that refuses a box shadow so that the absent form of a declaration is exercised by the tree that has to pass. |

## The C2 exceptions

| blocklist entry | why |
|---|---|
| `Roboto` | The stack used to name Android's interface face in the position an Android system reads it, and this row is what is left behind after the face that needed it was taken out of the stylesheet. |
