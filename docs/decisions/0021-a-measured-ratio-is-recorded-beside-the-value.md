# 0021: a measured contrast ratio is recorded beside the value

- Status: decided
- Recorded by: S0063-chorus-styling-tokens, against the umbrella's
  `.sdd/conventions/styling.md` clause S7
- Implemented in: `crates/server/src/ui/tokens.css`, `tools/ui/reconcile.js`,
  `tools/ui/ui.spec.js`, `tools/ui/styling-scan.js`

## The question

The control page's stylesheet used to carry this argument in its header:

> Every pair below that has to clear a floor is measured in the engine, in BOTH
> themes, by tools/ui/contrast.js - not read back from these declarations, which
> is why there is no ratio written beside a value here that could go stale.

The reasoning was sound as far as it went. A number a person types beside a
colour is a number nobody re-derives: change the colour, forget the comment, and
the file now states something false with the authority of having been written
down. A measurement in an engine cannot go stale that way, because it is taken
again on every run.

The umbrella's styling conventions ask for the opposite. S7: "Every token pair
that must clear a floor carries its measured ratio, in both themes, next to the
token. A later change that drops below a floor is then visible in the diff."
What that buys is the thing an engine measurement cannot: a reviewer reading a
diff sees the number move, at the moment the change is proposed, rather than
learning it from a build afterwards.

Both are true. The question is what to do about the objection the header raised,
because a file that carries the annotations AND the argument against them is
worse than either.

Decision: a measured contrast ratio is recorded beside each token value in
`crates/server/src/ui/tokens.css`, in both themes, and the objection is answered
by making the recorded number impossible to leave stale.

Supersedes: the rule stated in the old `chorus.css` header, that no ratio is
written beside a value here because it could go stale. That header claim is
removed by the same change that adds the annotations, and
`tools/styling-check.sh` refuses a stylesheet comment that still asserts it.

Enforcement: a stale annotation is caught by the reconciliation in
`make verify-ui`, not by a reader. On every run, in both themes, the rendered
check maps every row its contrast measurement produced back to the token pair
that painted it and compares the recorded number against the measured one; a
difference of more than 0.05 exits non-zero naming the pair, the theme, the
number on record and the number measured. A measured row that maps to no token
pair, and a mapped pair with no recorded ratio for that theme, are both refusals
too, so a reconciliation that covered only part of what the engine measured
cannot report itself green. Every recorded pair is also checked against the
values the engine resolved the two tokens to, which covers a pair the page did
not happen to paint in that run.

## What this costs

An annotation is a second place a number lives, and the reconciliation is the
machinery that keeps the two honest. That machinery is the price. It is paid
once, it runs on every `make verify-ui`, and it is the difference between an
annotation that is evidence and an annotation that is decoration.

## What it does not change

Nothing about how a colour is CHOSEN or how a floor is judged. The floors are
still WCAG 2.2 AA and they are still measured off the framebuffer by
`tools/ui/contrast.js`; the recorded numbers are a record OF that measurement
and never an input to it. No check reads a ratio out of the stylesheet and
reports a floor as cleared.
