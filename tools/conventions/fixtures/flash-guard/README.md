# Flash-guard fixtures

Read by `tools/conventions/check-flash-guard-fixtures.sh`. Every file under `forbidden/` holds
one way to set, export or bypass the owner-at-bench variable and must make
`check-flash-guard.sh` fail when it is the only file scanned; every file under `allowed/`
holds approved read forms (or a setting under `docs/`) and must pass. The file path under each
directory is the path the scanner sees, so `forbidden/.claude/settings.json` is scanned as
`.claude/settings.json`.

The first twelve forbidden forms are the ones the planning review of the brief found
([`.claude/goals/2026-09-chorus-research/review-brief-v1.md`](https://github.com/NSchatz/chorus/blob/535ed2816b5735153ac81c52a235024aa806f2b5/.claude/goals/2026-09-chorus-research/review-brief-v1.md), H2); the rest are further ways
to set it in the languages and files this repository uses. Forms 24-36 (chorus goal 9) are
the ones goal 3's adversarial check found the scan passed: a C ternary default and a reassigned
read, Python `or` and `os.environ.get` defaults, Rust `map_or`, `.or(`, `unwrap_or_default` and
`is_err()`, a shell read assigned to a variable (and a two-line default), and the name built
from pieces three ways.
