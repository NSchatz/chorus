# pinning demonstrations

Seven small trees: six wrong on purpose and one right on purpose.
`tools/pinning-check.sh` scans each one and requires the six to go red naming
the clause of the umbrella's `documentation/pinning-conventions.md` that each
breaks, and the seventh to pass.

They exist because a check that has never been seen to fail is a formality
rather than evidence. Every text scanner is only as good as the thing it was
shown, and the way this repository keeps that honest elsewhere - the endpoint
safety scans, the rendered-UI mutations - is to commit the counterexample beside
the check and run it every time.

| directory | shape | clause | wanted exit |
|---|---|---|---|
| `from-without-digest` | a `FROM` with a tag and no digest | P2 | 2 |
| `action-at-a-mutable-tag` | a `uses:` at a mutable tag | P3 | 2 |
| `manifest-with-a-range` | a dependency manifest carrying a range | P4 | 2 |
| `lifecycle-scripts-back-on` | node lifecycle scripts re-enabled with no reason | P4 | 2 |
| `image-without-digest` | an image named outside a Dockerfile, no digest | P1 | 2 |
| `a-category-went-empty` | a tree in which a whole category matches nothing | - | 3 |
| `lifecycle-scripts-back-on-with-a-reason` | the same opt-back-in, with the reason committed beside it | P4 allows this | 0 |

The first five are scanned in `--fixture` mode, which judges pins and does not
ask whether every category was populated: a fixture populates one category on
purpose. `a-category-went-empty` is scanned as a repository, because the empty
category IS what it demonstrates.

The last one is the reason the fourth is not enough on its own. P4 does not
forbid turning node lifecycle scripts back on, it requires a committed reason
when you do, and a check that refused every `false` would leave evidence
identical to one that refuses only the unexplained ones. So the reasoned form is
committed too, and has to pass.

A reason has to be MARKED as one - `# reason: <why>` - rather than merely be a
comment sitting above the setting. The first version of this check accepted any
adjacent comment, and `tools/ui/pnpm-workspace.yaml` then passed with
`ignoreScripts: false` on the strength of its own instructions. The
`lifecycle-scripts-back-on` tree puts its comment block directly on top of the
setting for that reason: it is the case that used to slip through.

`tools/pinning-scan.sh` prunes this directory when it scans the repository. It
has to: these trees are unpinned by construction, and counting them against the
repository would leave chorus permanently red for the exact reason the check
exists to prevent. `tools/pinning-check.sh` checks that every directory here is
exercised, so one added and forgotten is a failure rather than a silence.

Nothing here is built, installed, deployed or run. They are read.
