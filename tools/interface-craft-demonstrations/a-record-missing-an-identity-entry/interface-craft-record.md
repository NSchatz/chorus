# A record that names four of the five identity entries

base/interface-craft-record.md with the radius signature row taken out. The four
that are left are correct and agree with `ui/tokens.css`, which is what makes
this the interesting case: a check that only graded the rows in front of it would
report four passes and say nothing about the one that is not there.

C1 names five and asks for one sentence on each. An unstated value is the gap the
training-data average fills, so a missing row is the fault the clause exists to
catch.

## The identity

| entry | declares | why |
|---|---|---|
| display face | `--face-prose` = `var(--font-ui)`; `--font-ui` = `"Chorus Display", -apple-system, sans-serif` | A repo-chosen face stands at the head of the stack, so this tree is not a bare system stack alone and the demonstration beside it that IS one differs in exactly that. |
| text face | `--face-prose` = `var(--font-ui)`; `--face-figure` = `var(--font-mono)`; `--font-mono` = `"Chorus Mono", ui-monospace, monospace` | Prose takes the display face and anything whose columns carry meaning takes the fixed-advance one, which is the two-role split the real record describes. |
| accent | `--accent` = `var(--teal-600)`; `--teal-600` = `#0a6b6b` | One hue, named for this tree rather than for a framework, so that the demonstrations which add an `indigo-500` or a `blue-600` beside it differ in exactly that. |
| shadow signature | absent; kept absent by `tools/ui/styling-scan.js` rule `border-and-surface-separation` | Nothing here is raised, and the entry names the committed rule that refuses a box shadow so that the absent form of a declaration is exercised by the tree that has to pass. |

## The C2 exceptions

| blocklist entry | why |
|---|---|
