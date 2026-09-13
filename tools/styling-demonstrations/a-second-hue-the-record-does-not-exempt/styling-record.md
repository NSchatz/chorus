# A styling record whose S10 row exempts no role

A copy of the table in `docs/styling-conventions-record.md` with ONE thing
wrong: S10's exemption gives a reason and names no role, so nothing is exempt.

This is the shape the shipped palette actually has, which is why it is committed
rather than argued about. The tokens here are `base`'s, unchanged: `--focus` is
an orange at high chroma roughly 177 degrees from the accent, exactly as it is
in `crates/server/src/ui/tokens.css`. Take the written exemption away and the
rule goes red on the real colour. That is what makes the exemption in the real
record load-bearing rather than decorative, and it is the case the sibling tree
`a-second-hue-outside-the-states` cannot show, because it puts its second hue on
`--mark`, a shape this palette does not have.

| clause | assertions | exemption | review |
|---|---|---|---|
| S1 | source:no-colour-literals, source:spacing-scale, source:tokens-resolve | | |
| S2 | source:role-vocabulary, source:three-tiers | | |
| S3 | source:spacing-scale | | |
| S4 | | Only the compact density is built. | |
| S5 | figures-fixed-advance, empty-notice-type-rule | | |
| S6 | source:hand-authored-themes, contrast-in-both-themes | | |
| S7 | tokens-reconciled | | |
| S8 | source:border-and-surface-separation, contrast-in-both-themes | | |
| S9 | | This page animates nothing. | |
| S10 | | The focus ring is neither the accent nor the line, and this cell names no role. | |
