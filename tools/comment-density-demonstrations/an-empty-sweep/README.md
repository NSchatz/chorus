# a tree with no Rust in it

There is no `.rs` file here and there is not meant to be. A sweep that matched
nothing has to be refused rather than reported as a compliant tree, because a
category that stopped matching and a tree with nothing wrong in it are the same
green otherwise.

`tools/pinning-demonstrations/a-category-went-empty` is the same shape for the
pinning gate.
