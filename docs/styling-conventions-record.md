# The styling conventions, clause by clause

The umbrella's `.sdd/conventions/styling.md` governs the token system every
surface in this umbrella is built from. chorus ships one surface: the control
page the server hands out on its control listener. This file says, for each
clause, which check proves it here, or why the clause does not reach this
surface.

It is not prose that can drift. `tools/ui/styling-scan.js` reads the table below
on every `make verify-styling` and exits non-zero, naming the clause, if any
clause is absent, carries more than one disposition, carries none, or names a
check that is not a check this tree runs.

A clause takes exactly one of three dispositions, and the checker admits exactly
these three:

- **an assertion**, named in the `assertions` column. A name beginning `source:`
  is a rule of `tools/ui/styling-scan.js`, which is a check of source text, and
  the checker refuses a name that is not a rule it ran in that invocation. Any
  other name is a rendered claim declared in `tools/ui/claims.js`, proved in a
  browser engine by `tools/ui/ui.spec.js` and shown going red from
  `tools/ui/mutation.spec.js`; `tools/ui/check-claims.js` is what refuses one
  that stopped running, on every `make verify-ui`.
- **an exemption**, with its reason, in the `exemption` column. An exemption is
  a claim about this surface and it has to stay true: the
  `source:exemptions-stay-true` rule refuses a stylesheet that reaches the thing
  the exemption says it does not reach.
- **the convention's own review disposition**, in the `review` column, for a
  clause whose own `*Graded:*` line assigns it to review rather than to a check.
  The cell has to quote that line. No clause of the styling conventions takes
  this today; the column exists because the disposition does, and the checker
  implements it rather than leaving a clause nowhere to go.

## The record

| clause | assertions | exemption | review |
|---|---|---|---|
| S1 | source:no-colour-literals, source:spacing-scale, source:tokens-resolve | | |
| S2 | source:role-vocabulary, source:three-tiers | | |
| S3 | source:spacing-scale | | |
| S4 | | Only the compact density is built. S0063's scope puts the second one out, and compact is the density the clause names as the default for operator tools. A second density is a doubled state matrix and every layout and contrast assertion run twice, which is its own piece of work; until it exists this surface has one density and no switch to choose it with. | |
| S5 | figures-fixed-advance, empty-notice-type-rule | | |
| S6 | source:hand-authored-themes, contrast-in-both-themes | | |
| S7 | tokens-reconciled | | |
| S8 | source:border-and-surface-separation, contrast-in-both-themes | | |
| S9 | | This page animates nothing. There is no transition, no animation and no keyframe in its stylesheet, so there is no motion to honour a reduced-motion preference about and no value change that is illegible with motion off. The moment one is added the exemption stops being true, which is what `source:exemptions-stay-true` refuses. | |
| S10 | | [exempts: --focus] One accent is claimed and declared, and every role outside the three state tokens the clause enumerates is neutral or the accent hue, with one exception written here instead of widened into the rule. The focus ring has to be neither the accent nor the line: this surface already spends the accent hue on the wordmark, on every link and on the fill of a pressed control, so a ring in that hue would be a ring the page is full of, and a ring in the line colour is the boundary it is drawn beside. The focus role is the one hue nothing else here carries, and it clears 3:1 against the panel in both themes off the painted pixels. `source:one-accent-hue` admits exactly the roles named in this cell, refuses a second hue on any other role, and refuses a name here that has stopped needing the exemption. | |

## Why each row says what it says

**S1 Every colour and length resolves to a token.** `source:no-colour-literals`
refuses a hexadecimal colour, an `rgb()`, `hsl()`, `oklch()` or any other colour
function, and every CSS named colour, in any stylesheet source outside the token
file. `source:spacing-scale` refuses a length literal in the same places, of any
unit, including the bare `0` a person writes without thinking of it as a length.
`source:tokens-resolve` is the other half of "resolves": a name a stylesheet
reaches for and no theme declares is a refusal, and so is a `var()` fallback,
which is how an undefined token stops being visible.

**S2 Three tiers, named for role.** `source:role-vocabulary` requires all
fifteen names - `--bg`, `--panel`, `--line`, `--fg`, `--muted`, `--accent`,
`--ok`, `--warn`, `--bad`, `--border`, `--mark`, `--focus`, `--disabled`,
`--selected`, `--link` - declared with an explicit value in both themes, and
refuses a sixteenth name smuggled into the semantic tier. `source:three-tiers`
holds the tiers apart: a stylesheet source may name a semantic role or a
component token and never a primitive, and a component token binds a role rather
than reaching past it into the palette.

The tier rule is applied to colour exactly as the clause states it: primitives
are named only by the semantic tier, and the surface names only the semantic and
component tiers. A colour is a colour in any of the forms one is written in, so
the rule refuses a component reaching for a primitive whose value is a hex, a
colour function or a CSS named colour; a rule that only saw the hexadecimal form
would report "no component token names a primitive colour" over a component
naming a keyword. The absence of paint is not in the palette and so is not a
primitive: `--nothing` is declared in the component tier, where the two slider
rules that have to undo the shared control surface and edge can name it without
reaching past a role for a colour. For lengths and type it is applied one tier
shorter, because
the clause's semantic vocabulary is a colour vocabulary and there is no semantic
length role to route a padding through: the 4px scale and the type scale are
primitives, the component tier binds the step each component spends, and the
surface still names only component tokens. What the check enforces either way is
the property the clause is for - no rule in a stylesheet reaches past a name that
says what the value is FOR.

**S3 Spacing is the 4px scale.** `source:spacing-scale` resolves every token a
spacing, padding, margin, gap or size property spends, in both themes, and
refuses one that is not a whole multiple of 4px or is not written in whole
pixels. Properties outside that set - a hairline, a focus ring's width and
offset, a corner radius, a type size - are held to the first half of the rule
and not to the scale: they may not carry a literal, and a 4px hairline would be
a slab.

**S4 Two densities.** Exempt, with the reason in the row. The one density this
page has is the compact one.

**S5 Data is monospace, prose and controls are system sans.** Graded in the
engine, twice. `figures-fixed-advance` lays two equal-length runs into every run
whose column alignment carries meaning - the volume figure, the endpoint figure,
the footer's serial line, and a heading showing an identifier because no name
could be read - and requires the engine to paint them at the same width, then
does the same to every heading, sentence and control label and requires
different widths. `empty-notice-type-rule` is the same measurement on the state
a server with no zone configured serves, so the empty state is held to the type
rule the populated one is.

**S6 Each theme is hand-authored.** `source:hand-authored-themes` requires every
one of the fifteen roles, in each theme, to name one primitive chosen for that
theme, and refuses a `color-mix()`, a relative-colour form, a filter and a role
that names another role. `contrast-in-both-themes` is the other half the clause
asks for: the floors are measured off the painted pixels once per theme, so a
palette that was chosen rather than derived is also a palette that was proved.

**S7 Measured contrast is recorded beside the value.** `tokens-reconciled` maps
every row the contrast measurement produced back to the token pair that painted
it, by the colours the engine resolved those tokens to, and compares the ratio
recorded in the token file against the ratio measured from the framebuffer. A
difference over 0.05 is a refusal naming the pair, the theme and both numbers; a
measured row that maps to no token pair is a refusal; a mapped pair carrying no
recorded ratio for that theme is a refusal. `docs/decisions/0021-a-measured-ratio-is-recorded-beside-the-value.md`
records why the annotations are there at all and what rule they replace.

**S8 Depth is borders plus a small shadow scale.** The half of the clause this
surface reaches is asserted: `source:border-and-surface-separation` requires the
separation on this page to be a border token and a surface token, and refuses a
`box-shadow` or a `text-shadow`, and `contrast-in-both-themes` measures every
control boundary against the surface behind it at 3:1 in both themes. The other
half - two or three shadow tokens for genuinely raised surfaces - is absent, and
absent on purpose: this page raises nothing. There is no menu and no dialog on
it, and a shadow scale for surfaces that do not exist would be a palette nobody
had to choose. The rule that refuses a shadow is what keeps that true: the first
raised surface makes this build red, and whoever adds it adds the scale with it.

**S9 Motion is decoration, never information.** Exempt, with the reason in the
row, and the exemption is machine-checked rather than asserted in prose.

**S10 The accent is the only non-neutral hue.** Exempt for one named role, with
the reason in the row, and the exemption is machine-checked rather than asserted
in prose. `source:one-accent-hue` resolves every role in each theme, measures how
far each is from grey, and requires every one that has a hue at all to be within
fifteen degrees of `--accent`. Its exception list is the clause's own
enumeration and nothing else: `--ok`, `--warn` and `--bad`. `--focus`,
`--disabled` and `--selected` are state roles in the S2 sense and the clause
still does not name them, so they are measured like any other role - `--selected`
and `--link` are the accent hue, `--disabled` is a neutral, and `--focus` is the
one role that needs the exemption the row writes down. The rule reads that row
and admits exactly the roles it names, which is the half that keeps an exemption
from quietly becoming a list: a second hue on a role the row does not name is a
refusal, and so is a name in the row that is already inside the accent window.

There is no third route. An exception list held in the checker rather than in
this row would report a conformance nobody measured, which is the failure mode
`frontend.md` F1 names: a check whose exception list is where the disagreement
with the clause was put.

## What is not in this table

`tools/ui/styling-scan.js` runs six rules no clause cites. `tokens-parse`
refuses a token file it cannot read rather than reading less of it.
`clause-record` is this file being read. `decision-record` is the decision above
being required and no stylesheet comment being allowed to contradict it.
`exemptions-stay-true` holds S4's and S9's exemptions to their own premises, and
`one-accent-hue` holds S10's to its, which is why S10's row is an exemption and
not an assertion: a rule that reads a row cannot also be that row's disposition
without grading itself. `styling-stays-in-the-stylesheet` is what makes the
`.css` files the WHOLE source set the first three rules are written over: a
`<style>` block, a `style=` attribute or an `element.style` written from the
script would be styling no stylesheet check could reach, so none of them is
allowed to exist and the page says what it looks like by naming a class. They
are the machinery that keeps the table honest rather than dispositions of a
clause.
