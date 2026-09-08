# pinning demonstrations

Six small trees that are wrong on purpose. `tools/pinning-check.sh` scans each
one and requires it to go red naming the clause of the umbrella's
`documentation/pinning-conventions.md` that it breaks.

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

The first five are scanned in `--fixture` mode, which judges pins and does not
ask whether every category was populated: a fixture populates one category on
purpose. `a-category-went-empty` is scanned as a repository, because the empty
category IS what it demonstrates.

`tools/pinning-scan.sh` prunes this directory when it scans the repository. It
has to: these trees are unpinned by construction, and counting them against the
repository would leave chorus permanently red for the exact reason the check
exists to prevent. `tools/pinning-check.sh` checks that every directory here is
exercised, so one added and forgotten is a failure rather than a silence.

Nothing here is built, installed, deployed or run. They are read.
